//! Serialize macOS launches until the single-instance plugin is listening.
//! Its socket is bound asynchronously; without this guard simultaneous launches
//! can both decide that they are the first instance.

use std::{
    fs::File,
    io,
    os::unix::net::UnixStream,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

pub struct StartupLock {
    _file: File,
    socket_path: PathBuf,
}

impl StartupLock {
    pub fn acquire(identifier: &str) -> io::Result<Self> {
        let identifier = identifier.replace(['.', '-'], "_");
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(std::env::temp_dir().join(format!("{identifier}_startup.lock")))?;
        file.lock()?;
        Ok(Self {
            _file: file,
            // Matches tauri-plugin-single-instance's macOS socket path.
            socket_path: PathBuf::from(format!("/tmp/{identifier}_si.sock")),
        })
    }

    pub fn finish(self) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match UnixStream::connect(&self.socket_path) {
                Ok(stream) => {
                    drop(stream);
                    return Ok(());
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                    ) && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        }
        // Dropping the file releases the OS lock, including after a crash.
        // Keep the empty file: unlinking it could split waiters across inodes.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::net::UnixListener,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn identifier() -> String {
        format!(
            "carnyx_test_{}_{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[test]
    fn simultaneous_launch_waits_until_the_first_releases_its_lock() {
        let id = identifier();
        let first = StartupLock::acquire(&id).unwrap();
        let (sender, receiver) = mpsc::channel();
        let (started_sender, started_receiver) = mpsc::channel();
        let second_id = id.clone();
        let second = thread::spawn(move || {
            started_sender.send(()).unwrap();
            let guard = StartupLock::acquire(&second_id).unwrap();
            sender.send(()).unwrap();
            guard
        });
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        drop(first);
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(second.join().unwrap());
        // A leftover lock file must not prevent a later restart.
        drop(StartupLock::acquire(&id).unwrap());
        fs::remove_file(std::env::temp_dir().join(format!("{id}_startup.lock"))).unwrap();
    }

    #[test]
    fn finish_waits_for_the_plugin_listener_to_be_ready() {
        let id = identifier();
        let guard = StartupLock::acquire(&id).unwrap();
        let socket = guard.socket_path.clone();
        let listener_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            let listener = UnixListener::bind(&socket).unwrap();
            listener.accept().unwrap();
            fs::remove_file(socket).unwrap();
        });
        guard.finish().unwrap();
        listener_thread.join().unwrap();
        fs::remove_file(std::env::temp_dir().join(format!("{id}_startup.lock"))).unwrap();
    }

    #[test]
    fn missing_listener_fails_startup_and_releases_the_lock() {
        let id = identifier();
        let guard = StartupLock::acquire(&id).unwrap();
        assert_eq!(guard.finish().unwrap_err().kind(), io::ErrorKind::NotFound);
        drop(StartupLock::acquire(&id).unwrap());
        fs::remove_file(std::env::temp_dir().join(format!("{id}_startup.lock"))).unwrap();
    }
}
