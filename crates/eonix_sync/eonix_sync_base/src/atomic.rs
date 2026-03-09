#[macro_export]
macro_rules! atomic {
    (@$order:ident, $e:expr, $func:ident) => {{
        use core::sync::atomic::Ordering;

        $e.$func(Ordering::$order)
    }};

    (@$order:ident, $e:expr, $func:ident, $($arg:expr),*) => {{
        use core::sync::atomic::Ordering;

        $e.$func($($arg),*, Ordering::$order)
    }};

    ($e:expr, $func:ident) => {
        $crate::atomic!(@SeqCst, $e, $func)
    };

    ($e:expr, $func:ident, $($arg:expr),*) => {
        $crate::atomic!(@SeqCst, $e, $func, $($arg),*)
    };
}
