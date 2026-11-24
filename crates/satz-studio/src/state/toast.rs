//! What the stores have to say to the operator, waiting to be shown: the window takes
//! each one and shows it as a toast.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
    /// a warning the drawer lists as a message of the estate; counted beside the errors
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
}
