//! Prepared WP-0230b numeric core only. Never infers installed status or network bytes from disk.
use serde::Serialize;
use std::time::Duration;

const MAX_LINE: usize = 4096;
const MAX_EXACT_BYTES: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TransferSnapshot {
    pub generation: u64,
    pub received_bytes: u64,
    pub baseline_bytes: u64,
    pub newly_received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: Option<f64>,
    pub remaining_transfer_seconds: Option<f64>,
}

/// A parser belongs to one original root/job/attempt/step/child-command scope.
/// The caller clears it on command/step/attempt termination; it is not durable state.
#[derive(Default)]
pub struct PipRawDecoder {
    line: Vec<u8>,
    overflow: bool,
    generation: u64,
    prior: Option<(u64, Option<u64>, u64, Duration)>,
    latest: Option<TransferSnapshot>,
}

impl PipRawDecoder {
    pub fn clear(&mut self) { self.line.clear(); self.overflow=false; self.prior=None; self.latest=None; }
    /// Reader coalescing loss invalidates rate/baseline before any later observation.
    pub fn note_gap(&mut self) { self.clear(); }
    pub fn feed(&mut self, bytes: &[u8], observed_at: Duration) {
        for &byte in bytes {
            if byte == b'\n' || byte == b'\r' {
                if !self.overflow { self.parse_line(observed_at); }
                self.line.clear(); self.overflow=false;
            } else if self.line.len() < MAX_LINE && !self.overflow {
                self.line.push(byte);
            } else {
                if self.line.starts_with(b"Progress ") { self.prior=None; self.latest=None; }
                self.line.clear(); self.overflow=true;
            }
        }
    }
    fn parse_line(&mut self, at: Duration) {
        if !self.line.starts_with(b"Progress ") { return; }
        let Ok(line)=std::str::from_utf8(&self.line) else { self.prior=None;self.latest=None;return; };
        let Some((current,total))=line.strip_prefix("Progress ").and_then(|v|v.split_once(" of ")) else { self.prior=None;self.latest=None;return; };
        let integer=|value:&str| -> Option<u64> {
            if value.is_empty() || !value.bytes().all(|v|v.is_ascii_digit()) { return None; }
            value.parse::<u64>().ok().filter(|v|*v<=MAX_EXACT_BYTES)
        };
        let Some(current)=integer(current) else { self.prior=None;self.latest=None;return; };
        let Some(total)=integer(total) else { self.prior=None;self.latest=None;return; };
        let total=(total>0).then_some(total);
        if total.is_some_and(|total|current>total) { self.prior=None;self.latest=None;return; }
        let (baseline,rate)=match self.prior {
            Some((previous,old_total,baseline,old_at)) if current>=previous && total==old_total && at>old_at => {
                let seconds=(at-old_at).as_secs_f64();
                (baseline,((current-previous)>0).then(||(current-previous) as f64/seconds))
            },
            // A new raw counter, resume baseline, changed total, or nonmonotonic clock is never a speed sample.
            _ => { self.generation=self.generation.saturating_add(1); (current,None) },
        };
        let rate=rate.filter(|v|v.is_finite() && *v>0.0);
        let eta=total.zip(rate).map(|(total,rate)|(total-current) as f64/rate).filter(|v|v.is_finite() && *v>=0.0);
        self.latest=Some(TransferSnapshot {generation:self.generation,received_bytes:current,baseline_bytes:baseline,newly_received_bytes:current-baseline,total_bytes:total,bytes_per_second:rate,remaining_transfer_seconds:eta});
        self.prior=Some((current,total,baseline,at));
    }
    /// Missing/new-process state remains None. Stale measurements retain byte observations only.
    pub fn snapshot(&self, now: Duration) -> Option<TransferSnapshot> {
        let mut value=self.latest.clone()?;
        if self.prior.is_none_or(|(_,_,_,at)|now<at || now-at>Duration::from_secs(3)) {
            value.bytes_per_second=None;value.remaining_transfer_seconds=None;
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunked_raw_protocol_has_exact_bytes_and_unknown_total_has_no_eta() {
        let mut d=PipRawDecoder::default();
        d.feed(b"Progress 0 of 0\nProgress 1",Duration::ZERO);
        d.feed(b"00 of 0\n",Duration::from_secs(2));
        let value=d.snapshot(Duration::from_secs(2)).unwrap();
        assert_eq!(value.received_bytes,100);assert_eq!(value.bytes_per_second,Some(50.0));
        assert_eq!(value.total_bytes,None);assert_eq!(value.remaining_transfer_seconds,None);
    }
    #[test]
    fn resume_reset_and_gap_do_not_count_existing_bytes_as_new_transfer() {
        let mut d=PipRawDecoder::default();
        d.feed(b"Progress 600 of 1000\n",Duration::ZERO);
        assert_eq!(d.snapshot(Duration::ZERO).unwrap().newly_received_bytes,0);
        d.feed(b"Progress 800 of 1000\n",Duration::from_secs(2));
        let value=d.snapshot(Duration::from_secs(2)).unwrap();
        assert_eq!(value.newly_received_bytes,200);assert_eq!(value.remaining_transfer_seconds,Some(2.0));
        d.feed(b"Progress 0 of 1000\n",Duration::from_secs(3));
        assert_eq!(d.snapshot(Duration::from_secs(3)).unwrap().bytes_per_second,None);
        d.note_gap();d.feed(b"Progress 900 of 1000\n",Duration::from_secs(4));
        assert_eq!(d.snapshot(Duration::from_secs(4)).unwrap().newly_received_bytes,0);
    }
    #[test]
    fn malformed_oversized_and_stale_frames_cannot_supply_eta() {
        let mut d=PipRawDecoder::default();
        d.feed(b"Progress 0 of 100\n",Duration::ZERO);
        d.feed(b"Progress 50 of 100\n",Duration::from_secs(1));
        assert_eq!(d.snapshot(Duration::from_secs(5)).unwrap().bytes_per_second,None);
        d.feed(b"Progress 101 of 100\n",Duration::from_secs(6));assert!(d.snapshot(Duration::from_secs(6)).is_none());
        d.feed(&vec![b'x';MAX_LINE+1],Duration::from_secs(7));
        d.feed(b"Progress 1 of 2\n",Duration::from_secs(8));assert!(d.snapshot(Duration::from_secs(8)).is_none());
    }
    #[test]
    fn malformed_eligible_utf8_clears_rate_but_unrelated_output_does_not() {
        let mut d=PipRawDecoder::default();
        d.feed(b"Progress 0 of 100\n",Duration::ZERO);
        d.feed(b"Progress 50 of 100\n",Duration::from_secs(1));
        d.feed(b"unrelated \xff message\n",Duration::from_secs(1));
        assert_eq!(d.snapshot(Duration::from_secs(1)).unwrap().bytes_per_second,Some(50.0));
        d.feed(&vec![b'x';MAX_LINE+1],Duration::from_secs(1));d.feed(b"\n",Duration::from_secs(1));
        assert_eq!(d.snapshot(Duration::from_secs(1)).unwrap().bytes_per_second,Some(50.0));
        d.feed(b"Progress \xff of 100\n",Duration::from_secs(2));
        assert!(d.snapshot(Duration::from_secs(2)).is_none());
        d.feed(b"Progress 75 of 100\n",Duration::from_secs(3));
        let value=d.snapshot(Duration::from_secs(3)).unwrap();
        assert_eq!(value.bytes_per_second,None);assert_eq!(value.newly_received_bytes,0);
        d.feed(b"Progress ",Duration::from_secs(3));d.feed(&vec![b'1';MAX_LINE],Duration::from_secs(3));
        d.feed(b"\n",Duration::from_secs(3));assert!(d.snapshot(Duration::from_secs(3)).is_none());
    }
}
