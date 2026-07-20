use std::sync::Arc;

pub(crate) trait RuntimeWake: Send + Sync {
    fn notify(&self);
}

#[derive(Clone, Default)]
pub(crate) struct WakeHandle(Option<Arc<dyn RuntimeWake>>);

impl WakeHandle {
    pub(crate) fn new<W>(wake: Arc<W>) -> Self
    where
        W: RuntimeWake + 'static,
    {
        Self(Some(wake))
    }

    pub(crate) fn notify(&self) {
        if let Some(wake) = &self.0 {
            wake.notify();
        }
    }
}
