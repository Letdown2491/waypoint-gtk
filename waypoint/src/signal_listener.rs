// D-Bus signal listener for snapshot events

use anyhow::Result;
use futures_util::StreamExt;
use gtk::glib;
use gtk::Application;
use waypoint_common::*;
use zbus::{Connection, MatchRule};

use crate::ui::notifications;

#[derive(Clone, Debug)]
pub struct SnapshotCreatedEvent {
    pub snapshot_name: String,
    pub created_by: String,
}

/// Start listening for snapshot creation signals from waypoint-helper
///
/// This function spawns an async task that listens for D-Bus signals and
/// sends desktop notifications when snapshots are created by the scheduler.
pub fn start_signal_listener(app: Application) {
    // Create a channel for thread-safe communication
    let (sender, receiver) = std::sync::mpsc::channel();

    // Spawn a separate thread for async D-Bus signal listening
    std::thread::spawn(move || {
        // Run the async listener
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            if let Err(e) = listen_for_signals(sender).await {
                log::error!("Signal listener error: {}", e);
            }
        });
    });

    // Set up receiver on main GTK thread
    glib::spawn_future_local(async move {
        loop {
            if let Ok(event) = receiver.try_recv() {
                println!("Main thread received: {:?}", event);

                // Only send notification if created by scheduler
                if event.created_by == "scheduler" {
                    notifications::notify_scheduled_snapshot(&app, &event.snapshot_name);
                }
            }

            // Sleep briefly to avoid busy waiting
            glib::timeout_future(std::time::Duration::from_millis(100)).await;
        }
    });
}

/// Async function to listen for snapshot_created signals
async fn listen_for_signals(sender: std::sync::mpsc::Sender<SnapshotCreatedEvent>) -> Result<()> {
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
    ).await?;

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
                    if member_name.as_str() == "SnapshotCreated" {
                        // Parse signal arguments - expecting (String, String)
                        if let Ok((snapshot_name, created_by)) = msg.body().deserialize::<(String, String)>() {
                            println!("Received SnapshotCreated signal: {} (by {})", snapshot_name, created_by);

                            // Send event to main thread
                            let event = SnapshotCreatedEvent {
                                snapshot_name,
                                created_by,
                            };

                            if let Err(e) = sender.send(event) {
                                log::error!("Failed to send event to main thread: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
