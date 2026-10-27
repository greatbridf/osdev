use core::ops::Deref;
use core::pin::Pin;

pub trait Processor {
    fn local() -> impl Deref<Target = Pin<&'static mut Self>>
    where
        Self: 'static;
}
