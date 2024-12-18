use strum_macros::EnumIter;

#[derive(EnumIter, Eq, PartialEq)]
pub enum Unit {
    Unit,
}

pub trait MaybeToString {
    fn maybe_to_string(&self) -> Option<String>;
}

impl MaybeToString for Unit {
    fn maybe_to_string(&self) -> Option<String> {
        None
    }
}

impl<T> MaybeToString for T
where
    T: ToString,
{
    fn maybe_to_string(&self) -> Option<String> {
        Some(self.to_string())
    }
}
