cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        mod x86_64;
        pub use x86_64::*;
    } else {
        use eonix_mm::address::VAddr;
        use posix_types::ctypes::PtrT;

        use crate::prelude::KResult;


        #[derive(Debug, Clone)]
        pub struct UserTLS(VAddr);

        #[derive(Debug, Clone)]
        pub struct UserTLSDescriptor(VAddr);

        impl UserTLS {
            pub fn activate(&self) {
                self.0;
            }
        }

        impl UserTLSDescriptor {
            pub fn new(tp: PtrT) -> KResult<Self> {
                Ok(Self(VAddr::from(tp.addr())))
            }

            pub fn read(&self) -> KResult<UserTLS> {
                Ok(UserTLS(self.0))
            }
        }
    }
}
