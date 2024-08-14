use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Inner {
        queue: VecDeque::default(),
        senders: 1,
    };

    let shared = Shared {
        inner: Mutex::new(inner),
        available: Condvar::new(),
    };

    let shared = Arc::new(shared);
    (
        Sender {
            shared: shared.clone(),
        },
        Receiver {
            shared: shared.clone(),
            buffer: VecDeque::new(),
        },
    )
}

////////////////// Receiver /////////////////////////////////
//////////////////          ////////////////////////////////

pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
    buffer: VecDeque<T>,
}

impl<T> Receiver<T> {
    pub fn recv(&mut self) -> Option<T> {
        if let Some(t) = self.buffer.pop_front() {
            return Some(t);
        }

        let mut inner = self.shared.inner.lock().unwrap();
        loop {
            match inner.queue.pop_front() {
                Some(t) => {
                    std::mem::swap(&mut self.buffer, &mut inner.queue);
                    return Some(t);
                }
                // If no item has arrived yet, we reassign the queue and
                // pause the current thread until notified by the Condvar
                None if inner.senders == 0 => return None,
                None => {
                    inner = self.shared.available.wait(inner).unwrap();
                }
            }
        }
    }
}

impl<T> Iterator for Receiver<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
    }
}

////////////////// Sender /////////////////////////////////
//////////////////       /////////////////////////////////

pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Sender<T> {
    pub fn send(&self, t: T) {
        let mut inner = self.shared.inner.lock().unwrap();
        inner.queue.push_back(t);
        // Drop the lock so the other thread can immediately take the lock
        drop(inner);
        // Notify the the thread to wake up using the Condvar
        self.shared.available.notify_one();
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        let mut inner = self.shared.inner.lock().unwrap();
        inner.senders += 1;
        drop(inner);

        Sender {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut inner = self.shared.inner.lock().unwrap();
        inner.senders -= 1;
        let was_last = inner.senders == 0;
        drop(inner);
        if was_last {
            self.shared.available.notify_one();
        }
    }
}

////////////////// Inner /////////////////////////////////
//////////////////      /////////////////////////////////

pub struct Inner<T> {
    queue: VecDeque<T>,
    senders: usize,
}

////////////////// Shared /////////////////////////////////
//////////////////       /////////////////////////////////

pub struct Shared<T> {
    inner: Mutex<Inner<T>>,
    available: Condvar,
}

////////////////// Tests /////////////////////////////////
//////////////////       /////////////////////////////////

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn send_string_across_threads() {
        let (tx, mut rx) = channel();
        let tx2 = tx.clone();

        thread::spawn(move || {
            tx.send("YOOOOOOOOOOO");
        });
        thread::spawn(move || {
            tx2.send("What Up");
        });

        assert_eq!(rx.next(), Some("YOOOOOOOOOOO"));
        assert_eq!(rx.next(), Some("What Up"));
    }

    #[test]
    fn ping_pong() {
        let (tx, mut rx) = channel();
        tx.send(42);
        let res = rx.recv();
        assert_eq!(res, Some(42));
        assert_ne!(res, Some(48));
    }

    #[test]
    fn closed() {
        let (tx, mut rx) = channel::<()>();
        drop(tx);
        assert_eq!(rx.recv(), None);
    }

    #[test]
    fn closed_rx() {
        let (tx, rx) = channel();
        drop(rx);
        tx.send(42);
    }
}
