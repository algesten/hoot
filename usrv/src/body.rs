pub enum Body {
    Empty,
}

impl From<()> for Body {
    fn from(_: ()) -> Self {
        Body::Empty
    }
}
