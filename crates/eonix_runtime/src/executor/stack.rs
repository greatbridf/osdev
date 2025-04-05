pub trait Stack: Sized + Send {
    fn new() -> Self;
    fn get_bottom(&self) -> &();
}
