use alloc::sync::Arc;
use core::task::Waker;
use eonix_sync::Spin;

enum OutputState<Output>
where
    Output: Send,
{
    Waiting(Option<Waker>),
    Finished(Option<Output>),
    TakenOut,
}

pub struct OutputHandle<Output>
where
    Output: Send,
{
    inner: OutputState<Output>,
}

impl<Output> OutputHandle<Output>
where
    Output: Send,
{
    pub fn new() -> Arc<Spin<Self>> {
        Arc::new(Spin::new(Self {
            inner: OutputState::Waiting(None),
        }))
    }

    pub fn try_resolve(&mut self) -> Option<Output> {
        let output = match &mut self.inner {
            OutputState::Waiting(_) => return None,
            OutputState::Finished(output) => output.take(),
            OutputState::TakenOut => panic!("Output already taken out"),
        };

        self.inner = OutputState::TakenOut;
        if let Some(output) = output {
            Some(output)
        } else {
            unreachable!("Output should be present")
        }
    }

    pub fn register_waiter(&mut self, waker: Waker) {
        if let OutputState::Waiting(inner_waker) = &mut self.inner {
            inner_waker.replace(waker);
        } else {
            panic!("Output is not waiting");
        }
    }

    pub fn commit_output(&mut self, output: Output) {
        if let OutputState::Waiting(inner_waker) = &mut self.inner {
            if let Some(waker) = inner_waker.take() {
                waker.wake();
            }
            self.inner = OutputState::Finished(Some(output));
        } else {
            panic!("Output is not waiting");
        }
    }
}
