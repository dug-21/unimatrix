// Faithful std::process::Command harness. modes: args <prog> <args..> | raw <prog> <rawline..>
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut c = Command::new(&a[1]);
    if a[0] == "raw" { for x in &a[2..] { c.raw_arg(x); } } else { c.args(&a[2..]); }
    c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut ch = match c.spawn() { Ok(x) => x, Err(e) => { eprintln!("spawn error: {e}"); std::process::exit(99) } };
    let mut si = ch.stdin.take().unwrap();
    let mut inp = String::new();
    std::io::stdin().read_to_string(&mut inp).ok();
    si.write_all(inp.as_bytes()).ok(); drop(si);
    let o = ch.wait_with_output().unwrap();
    std::io::stdout().write_all(&o.stdout).ok();
    std::io::stderr().write_all(&o.stderr).ok();
    std::process::exit(o.status.code().unwrap_or(98));
}
