#[path="canonical_stats_draft.rs"] mod stats;
fn main(){let mut args=std::env::args().skip(1);let input=args.next().unwrap();let output=args.next().unwrap();let bytes=std::fs::read(input).unwrap();let observed=stats::instrument(&bytes);std::fs::write(output,observed).unwrap();println!("read-only component observer validates");}
