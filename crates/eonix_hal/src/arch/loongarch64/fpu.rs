use core::arch::asm;
use eonix_hal_traits::fpu::RawFpuState;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FpuState {
    pub fx: [u64; 32],
    pub fcc: u64,
    pub fcsr: u64,
}

impl RawFpuState for FpuState {
    fn new() -> Self {
        unsafe { core::mem::zeroed() }
    }

    /// Save reg -> mem
    fn save(&mut self) {
        unsafe {
            asm!(
            "fst.d $f0,  {base},  0 * 8",
            "fst.d $f1,  {base},  1 * 8",
            "fst.d $f2,  {base},  2 * 8",
            "fst.d $f3,  {base},  3 * 8",
            "fst.d $f4,  {base},  4 * 8",
            "fst.d $f5,  {base},  5 * 8",
            "fst.d $f6,  {base},  6 * 8",
            "fst.d $f7,  {base},  7 * 8",
            "fst.d $f8,  {base},  8 * 8",
            "fst.d $f9,  {base},  9 * 8",
            "fst.d $f10, {base}, 10 * 8",
            "fst.d $f11, {base}, 11 * 8",
            "fst.d $f12, {base}, 12 * 8",
            "fst.d $f13, {base}, 13 * 8",
            "fst.d $f14, {base}, 14 * 8",
            "fst.d $f15, {base}, 15 * 8",
            "fst.d $f16, {base}, 16 * 8",
            "fst.d $f17, {base}, 17 * 8",
            "fst.d $f18, {base}, 18 * 8",
            "fst.d $f19, {base}, 19 * 8",
            "fst.d $f20, {base}, 20 * 8",
            "fst.d $f21, {base}, 21 * 8",
            "fst.d $f22, {base}, 22 * 8",
            "fst.d $f23, {base}, 23 * 8",
            "fst.d $f24, {base}, 24 * 8",
            "fst.d $f25, {base}, 25 * 8",
            "fst.d $f26, {base}, 26 * 8",
            "fst.d $f27, {base}, 27 * 8",
            "fst.d $f28, {base}, 28 * 8",
            "fst.d $f29, {base}, 29 * 8",
            "fst.d $f30, {base}, 30 * 8",
            "fst.d $f31, {base}, 31 * 8",
            "",
            "movcf2gr    {tmp}, $fcc0",
            "move        {fcc}, {tmp}",
            "movcf2gr    {tmp}, $fcc1",
            "bstrins.d   {fcc}, {tmp}, 15, 8",
            "movcf2gr    {tmp}, $fcc2",
            "bstrins.d   {fcc}, {tmp}, 23, 16",
            "movcf2gr    {tmp}, $fcc3",
            "bstrins.d   {fcc}, {tmp}, 31, 24",
            "movcf2gr    {tmp}, $fcc4",
            "bstrins.d   {fcc}, {tmp}, 39, 32",
            "movcf2gr    {tmp}, $fcc5",
            "bstrins.d   {fcc}, {tmp}, 47, 40",
            "movcf2gr    {tmp}, $fcc6",
            "bstrins.d   {fcc}, {tmp}, 55, 48",
            "movcf2gr    {tmp}, $fcc7",
            "bstrins.d   {fcc}, {tmp}, 63, 56",
            "",
            "movfcsr2gr  {fcsr}, $fcsr0",
            base = in(reg) &raw mut self.fx,
            tmp = out(reg) _,
            fcc = out(reg) self.fcc,
            fcsr = out(reg) self.fcsr,
            options(nostack, preserves_flags));
        }
    }

    fn restore(&mut self) {
        unsafe {
            asm!(
            "fld.d $f0,  {base},  0 * 8",
            "fld.d $f1,  {base},  1 * 8",
            "fld.d $f2,  {base},  2 * 8",
            "fld.d $f3,  {base},  3 * 8",
            "fld.d $f4,  {base},  4 * 8",
            "fld.d $f5,  {base},  5 * 8",
            "fld.d $f6,  {base},  6 * 8",
            "fld.d $f7,  {base},  7 * 8",
            "fld.d $f8,  {base},  8 * 8",
            "fld.d $f9,  {base},  9 * 8",
            "fld.d $f10, {base}, 10 * 8",
            "fld.d $f11, {base}, 11 * 8",
            "fld.d $f12, {base}, 12 * 8",
            "fld.d $f13, {base}, 13 * 8",
            "fld.d $f14, {base}, 14 * 8",
            "fld.d $f15, {base}, 15 * 8",
            "fld.d $f16, {base}, 16 * 8",
            "fld.d $f17, {base}, 17 * 8",
            "fld.d $f18, {base}, 18 * 8",
            "fld.d $f19, {base}, 19 * 8",
            "fld.d $f20, {base}, 20 * 8",
            "fld.d $f21, {base}, 21 * 8",
            "fld.d $f22, {base}, 22 * 8",
            "fld.d $f23, {base}, 23 * 8",
            "fld.d $f24, {base}, 24 * 8",
            "fld.d $f25, {base}, 25 * 8",
            "fld.d $f26, {base}, 26 * 8",
            "fld.d $f27, {base}, 27 * 8",
            "fld.d $f28, {base}, 28 * 8",
            "fld.d $f29, {base}, 29 * 8",
            "fld.d $f30, {base}, 30 * 8",
            "fld.d $f31, {base}, 31 * 8",
            "",
            "bstrpick.d  {tmp}, {fcc}, 7, 0",
            "movgr2cf    $fcc0, {tmp}",
            "bstrpick.d  {tmp}, {fcc}, 15, 8",
            "movgr2cf    $fcc1, {tmp}",
            "bstrpick.d  {tmp}, {fcc}, 23, 16",
            "movgr2cf    $fcc2, {tmp}",
            "bstrpick.d  {tmp}, {fcc}, 31, 24",
            "movgr2cf    $fcc3, {tmp}",
            "bstrpick.d  {tmp}, {fcc}, 39, 32",
            "movgr2cf    $fcc4, {tmp}",
            "bstrpick.d  {tmp}, {fcc}, 47, 40",
            "movgr2cf    $fcc5, {tmp}",
            "bstrpick.d  {tmp}, {fcc}, 55, 48",
            "movgr2cf    $fcc6, {tmp}",
            "bstrpick.d  {tmp}, {fcc}, 63, 56",
            "movgr2cf    $fcc7, {tmp}",
            "",
            "movgr2fcsr $fcsr0, {fcsr}",
            base = in(reg) &raw mut self.fx,
            fcc = in(reg) self.fcc,
            fcsr = in(reg) self.fcsr,
            tmp = out(reg) _,
            options(nostack, preserves_flags));
        }
    }
}
