//! Sleep / resume state machine. Windows may broadcast both automatic and user
//! resume events for the same wake. Schedule at most one catch-up update.
use std::time::{Duration,Instant};
pub const WAKE_SETTLE:Duration=Duration::from_secs(20);
const DUPLICATE_RESUME:Duration=Duration::from_secs(15);

#[derive(Default,Debug)]
pub struct WakeGate{
    sleeping:bool,
    pending:bool,
    last_resume:Option<Instant>,
}
impl WakeGate{
    pub fn suspend(&mut self)->bool{
        let first=!self.sleeping;
        self.sleeping=true;
        first
    }
    pub fn resume(&mut self,now:Instant)->bool{
        self.sleeping=false;
        if self.last_resume.is_some_and(|last|
            now.checked_duration_since(last).is_some_and(|d|d<DUPLICATE_RESUME)){
            return false;
        }
        self.last_resume=Some(now);
        self.pending=true;
        true
    }
    pub fn sleeping(&self)->bool{self.sleeping}
    pub fn pending(&self)->bool{self.pending}
    pub fn start_catchup(&mut self){self.pending=false;}
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn two_windows_resume_events_trigger_one_cycle(){
        let start=Instant::now();
        let mut gate=WakeGate::default();
        assert!(gate.suspend());
        assert!(gate.sleeping());
        assert!(gate.resume(start));
        assert!(!gate.resume(start+Duration::from_secs(2)));
        assert!(gate.pending());
        gate.start_catchup();
        assert!(!gate.pending());
        assert!(gate.resume(start+Duration::from_secs(17)));
        assert!(gate.pending());
    }
    #[test]fn repeated_suspend_is_idempotent(){
        let mut gate=WakeGate::default();
        assert!(gate.suspend());
        assert!(!gate.suspend());
    }
}
