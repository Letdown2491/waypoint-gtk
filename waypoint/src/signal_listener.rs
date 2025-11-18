// D-Bus signal listener for snapshot events

use anyhow::Result;
use futures_util::StreamExt;
use gtk::Application;
use gtk::glib;
use waypoint_common::*;
use zbus::{Connection, MatchRule};

use crate::ui::notifications;

#[derive(Clone, Debug)]
pub struct SnapshotCreatedEvent {
    pub snapshot_name: String,
    pub created_by: String,
}

#[derive(Clone, Debug)]
pub struct BackupProgressEvent {
    pub snapshot_id: String,
    pub destination_uuid: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: u64,
    pub stage: String,
}

#[derive(Clone, Debug)]
pub enum WaypointEvent {
    SnapshotCreated(SnapshotCreatedEvent),
    BackupProgress(BackupProgressEvent),
}

/// Start listening for waypoint-helper D-Bus signals
///
/// This function spawns an async task that listens for D-Bus signals and
/// sends desktop notifications when snapshots are created by the scheduler.
///
/// Returns a channel receiver for backup progress events
pub fn start_signal_listener(app: Application) -> std::sync::mpsc::Receiver<BackupProgressEvent> {
    // Create channels for thread-safe communication
    let (event_sender, event_receiver) = std::sync::mpsc::channel();
    let (progress_sender, progress_receiver) = std::sync::mpsc::channel();

    // Spawn a separate thread for async D-Bus signal listening
    std::thread::spawn(move || {
        // Run the async listener
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            if let Err(e) = listen_for_signals(event_sender).await {
                log::error!("Signal listener error: {}", e);
            }
        });
    });

    // Set up receiver on main GTK thread
    let progress_sender_clone = progress_sender.clone();
    glib::spawn_future_local(async move {
        loop {
            if let Ok(event) = event_receiver.try_recv() {
                match event {
                    WaypointEvent::SnapshotCreated(evt) => {
                        println!("Main thread received SnapshotCreated: {:?}", evt);

                        // Only send notification if created by scheduler
                        if evt.created_by == "scheduler" {
                            notifications::notify_scheduled_snapshot(&app, &evt.snapshot_name);
                        }
                    }
                    WaypointEvent::BackupProgress(evt) => {
                        println!("Main thread received BackupProgress: {:?}", evt);

                        // Forward to progress channel
                        if let Err(e) = progress_sender_clone.send(evt) {
                            log::error!("Failed to forward backup progress event: {}", e);
                        }
                    }
                }
            }

            // Sleep briefly to avoid busy waiting
            glib::timeout_future(std::time::Duration::from_millis(100)).await;
        }
    });

    progress_receiver
}

/// Async function to listen for waypoint-helper signals
async fn listen_for_signals(sender: std::sync::mpsc::Sender<WaypointEvent>) -> Result<()> {
    // Connect to system bus
    let connection = Connection::system().await?;

    // Create a match rule for the SnapshotCreated signal
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(DBUS_INTERFACE_NAME)?
        .member("SnapshotCreated")?
        .build();

    // Add match rule
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;

    let _: () = proxy.call("AddMatch", &(rule.to_string(),)).await?;

    println!("Signal listener started for snapshot_created signals");

    // Create a message stream
    let mut stream = zbus::MessageStream::from(&connection);

    // Listen for messages
    while let Some(msg) = stream.next().await {
        if let Ok(msg) = msg {
            // Check if this is our signal
            if msg.message_type() == zbus::message::Type::Signal {
                if let Some(member_name) = msg.header().member() {
                    match member_name.as_str() {
                        "SnapshotCreated" => {
                            // Parse signal arguments - expecting (String, String)
                            if let Ok((snapshot_name, created_by)) =
                                msg.body().deserialize::<(String, String)>()
                            {
                                println!(
                                    "Received SnapshotCreated signal: {} (by {})",
                                    snapshot_name, created_by
                                );

                                // Send event to main thread
                                let event = WaypointEvent::SnapshotCreated(SnapshotCreatedEvent {
                                    snapshot_name,
                                    created_by,
                                });

                                if let Err(e) = sender.send(event) {
                                    log::error!("Failed to send event to main thread: {}", e);
                                }
                            }
                        }
                        "BackupProgress" => {
                            // Parse signal arguments - expecting (String, String, u64, u64, u64, String)
                            if let Ok((snapshot_id, destination_uuid, bytes_transferred, total_bytes, speed_bytes_per_sec, stage)) =
                                msg.body().deserialize::<(String, String, u64, u64, u64, String)>()
                            {
                                println!(
                                    "Received BackupProgress signal: {} -> {} (stage: {})",
                                    snapshot_id, destination_uuid, stage
                                );

                                // Send event to main thread
                                let event = WaypointEvent::BackupProgress(BackupProgressEvent {
                                    snapshot_id,
                                    destination_uuid,
                                    bytes_transferred,
                                    total_bytes,
                                    speed_bytes_per_sec,
                                    stage,
                                });

                                if let Err(e) = sender.send(event) {
                                    log::error!("Failed to send event to main thread: {}", e);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}
