# Event-driven log viewer

This example shows a process-monitor-shaped application with no application UI
loop. Nagi owns terminal waiting, wake-up, message delivery, frame coalescing,
render timing, and subscription cancellation

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui --example log_viewer
```

The simulated process output is a long-lived `Subscription::stream` using
bounded Batch delivery. The one-second uptime is an independent
`Subscription::every` using Latest delivery. Both enter sequential `update`
calls, and Nagi renders their latest combined state at most once per allowed
frame. Application storage is a bounded `VecDeque`, while the virtual viewport
constructs only visible log rows

The sleep inside the simulated process adapter only makes the example
self-contained. A real adapter would block on process stdout and send a message
for each completed record. That source loop is supervised and cooperatively
cancelled by Nagi; it does not call `view`, request frames, or run a second UI
scheduler

The viewport follows output while it remains at the end. PageUp or the mouse
wheel leaves end-following without pausing input, and End resumes it. Press
Space or P to stop and restart the process-output subscription while uptime
continues. Q, Escape, or Control-C renders the final `STOPPED` state and exits

Source: [`main.rs`](main.rs)
