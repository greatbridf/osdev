mod execute_status;
mod output_handle;
mod stack;
mod stackful_executor;
mod stackless_executor;

pub use execute_status::ExecuteStatus;
pub use output_handle::OutputHandle;
pub use stack::Stack;
pub use stackful_executor::StackfulExecutorBuilder;
pub use stackless_executor::StacklessExecutorBuilder;

/// An `Executor` executes a `Run` object in a separate thread of execution
/// where we have a dedicated stack and context.
pub trait Executor: Send {
    fn progress(&self) -> ExecuteStatus;
}
