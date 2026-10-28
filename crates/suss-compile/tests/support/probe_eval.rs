use suss_compile::portable_session::{Session, SessionError, SessionValue};

/// Probes that construct built-in storage directly (`(new suss.core/X ...)`,
/// `(X. ...)`) run as core, which alone may do that (#279); every other probe
/// runs as user code, so the user path stays covered.
pub trait ProbeEval {
    fn eval_probe(&mut self, source: &str) -> Result<SessionValue, SessionError>;
}
impl ProbeEval for Session {
    fn eval_probe(&mut self, source: &str) -> Result<SessionValue, SessionError> {
        let constructs = source
            .split(|c: char| c.is_whitespace() || "()[]{}".contains(c))
            .any(|token| {
                token == "new"
                    || token.strip_suffix('.').is_some_and(|name| {
                        name.rsplit('/').next().and_then(|name| name.chars().next()).is_some_and(char::is_uppercase)
                    })
            });
        if constructs { self.eval_privileged(source) } else { self.eval(source) }
    }
}
