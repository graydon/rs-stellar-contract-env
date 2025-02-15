use core::fmt::Debug;
/// General trait representing a the ability of some object to perform a
/// (possibly unsuccessful) conversion between two other types.
pub trait Convert<F, T> {
    type Error: Debug;
    fn convert(&self, f: F) -> Result<T, Self::Error>;
}
