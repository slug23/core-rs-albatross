use futures::sync::BiLock;
use futures::{Async, Future};

pub enum ResponseError {
    InvalidValue,
    Timeout,
}

pub struct Response<T> {
    inner: BiLock<Option<Result<T, ResponseError>>>,
}

impl<T> Response<T> {
    pub fn new() -> (Self, Self) {
        let (lock1, lock2) = BiLock::new(None);
        let response1 = Response { inner: lock1 };
        let response2 = Response { inner: lock2 };
        (response1, response2)
    }

    /// Consumes this side of the lock, setting the final response.
    pub fn set(self, response: Result<T, ResponseError>) {
        // Block the thread until the lock is acquired.
        if let Ok(mut data) = self.inner.lock().wait() {
            *data = Some(response);
        }
    }
}

impl<T> Future for Response<T> {
    type Item = T;
    type Error = ResponseError;

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        match self.inner.poll_lock() {
            Async::Ready(mut guard) => {
                match guard.take() {
                    Some(inner) => inner.map(|data| Async::Ready(data)),
                    // Report not ready until the inner lock is filled.
                    None => Ok(Async::NotReady),
                }
            }
            Async::NotReady => Ok(Async::NotReady),
        }
    }
}
