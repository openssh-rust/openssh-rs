use super::super::Auxiliary;

use std::fmt;
use std::future::Future;
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio_util::sync::WaitForCancellationFuture;

use openssh_sftp_client::Error as SftpError;

const WAIT_FOR_CANCELLATION_FUTURE_SIZE: usize =
    mem::size_of::<WaitForCancellationFuture<'static>>();

#[derive(Default)]
pub(super) struct SelfRefWaitForCancellationFuture(
    /// WaitForCancellationFuture is erased to an array
    /// since it is a holds a reference to `Auxiliary::cancel_token`,
    /// which lives as long as `Self`.
    ///
    /// WaitForCancellationFuture is boxed since it stores an intrusive node
    /// inline, which is removed from waitlist on drop.
    ///
    /// However, in rust, leaking is permitted, thus we have to box it.
    Option<Pin<Box<[u8; WAIT_FOR_CANCELLATION_FUTURE_SIZE]>>>,
);

impl fmt::Debug for SelfRefWaitForCancellationFuture {
    fn fmt<'this>(&'this self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let future = self.0.as_ref().map(
            |reference: &Pin<Box<[u8; WAIT_FOR_CANCELLATION_FUTURE_SIZE]>>| {
                let reference: &[u8; WAIT_FOR_CANCELLATION_FUTURE_SIZE] = &*reference;

                let future: &WaitForCancellationFuture<'this> =
                    unsafe { mem::transmute(reference) };

                future
            },
        );

        f.debug_tuple("SelfRefWaitForCancellationFuture")
            .field(&future)
            .finish()
    }
}

impl SelfRefWaitForCancellationFuture {
    /// This function must be called once in `Drop` implementation.
    pub(super) unsafe fn drop<'this>(&'this mut self) {
        if let Some(boxed) = self.0.take() {
            // transmute the box to avoid moving `WaitForCancellationFuture`
            let _: Box<WaitForCancellationFuture<'this>> = mem::transmute(boxed);
        }
    }

    fn error() -> io::Error {
        io::Error::new(
            io::ErrorKind::Other,
            SftpError::BackgroundTaskFailure(&"read/flush task failed"),
        )
    }

    /// Return `Ok(())` if the task hasn't failed yet and the context has
    /// already been registered.
    pub(super) fn poll_for_task_failure<'this, 'auxiliary: 'this>(
        &'this mut self,
        cx: &mut Context<'_>,
        auxiliary: &'auxiliary Auxiliary,
    ) -> Result<(), io::Error> {
        if self.0.is_none() {
            let cancel_token = &auxiliary.cancel_token;

            if cancel_token.is_cancelled() {
                return Err(Self::error());
            }

            let future: WaitForCancellationFuture<'this> = cancel_token.cancelled();
            self.0 = Some(Box::pin(unsafe { mem::transmute(future) }));
        }

        {
            let reference: &mut Pin<Box<[u8; WAIT_FOR_CANCELLATION_FUTURE_SIZE]>> =
                self.0.as_mut().expect("self.0 is just set to Some");

            let reference: Pin<&mut [u8; WAIT_FOR_CANCELLATION_FUTURE_SIZE]> = Pin::new(reference);

            let future: Pin<&mut WaitForCancellationFuture<'this>> =
                unsafe { mem::transmute(reference) };

            match future.poll(cx) {
                Poll::Ready(_) => (),
                Poll::Pending => return Ok(()),
            }
        }

        self.0 = None;

        Err(Self::error())
    }
}
