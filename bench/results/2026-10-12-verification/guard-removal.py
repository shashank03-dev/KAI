# Remove each guard of docs/VERIFICATION.md on purpose, run the test that
# holds it, and put the guard back. Every line must print "FAILS (guard
# holds)". Run from the repo root: python3 bench/results/2026-10-12-verification/guard-removal.py
import subprocess, os
os.environ.setdefault('CARGO_INCREMENTAL', '0')
M=[
 ("gate check","crates/kai-core/src/engine/gate.rs","if !r.ledger.ledger.gated() || r.ledger.waived {","if true || !r.ledger.ledger.gated() || r.ledger.waived {",["--test","verify_e2e","done_is_refused"]),
 ("status from reply","crates/kai-core/src/ledger.rs","                    status: Status::Pending,","                    status: if obj.get(\"status\").and_then(|s| s.as_str()) == Some(\"verified\") { Status::Verified } else { Status::Pending },",["--test","verify_e2e","the_model_cannot_mark"]),
 ("refused probe drops outcome","crates/kai-core/src/ledger.rs","                        Err(e) => (None, Some(e)),","                        Err(_) => continue,",["--lib","a_refused_probe_keeps"]),
 ("allowlist","crates/kai-core/src/shell_ro.rs","    if POSIX.contains(&program) {","    if true || POSIX.contains(&program) {",["--test","verify_e2e","a_write_never"]),
 ("allowlist (lib)","crates/kai-core/src/shell_ro.rs","        if let Some(c) = cmd.chars().find(|c| FORBIDDEN.contains(c)) {","        if let Some(c) = cmd.chars().find(|_| false) {",["--lib","writes_and_anything"]),
 ("timeout","crates/kai-core/src/engine/gate.rs","match tokio::time::timeout(limit, port.check(probe)).await {","match Ok::<_, ()>(port.check(probe).await) {",["--test","verify_e2e","a_hanging_probe"]),
 ("bound","crates/kai-core/src/engine/gate.rs","        if r.ledger.rejections < MAX_GATE_REJECTIONS {","        if r.ledger.rejections < u32::MAX {",["--test","verify_e2e","gate_refusals_are_bounded"]),
 ("bound headless","crates/kai-core/src/engine/gate.rs","        if r.ledger.rejections < MAX_GATE_REJECTIONS {","        if r.ledger.rejections < u32::MAX {",["--test","verify_e2e","headless_gate"]),
 ("waiver","crates/kai-core/src/engine/gate.rs","            r.ledger.waived = true;","            r.ledger.waived = false;",["--test","verify_e2e","i_did_it"]),
 ("redaction","crates/kai-core/src/ledger.rs","    let mut out = s.to_string();\n    for c in codes {","    let mut out = s.to_string();\n    for c in codes.iter().take(0) {",["--test","verify_e2e","nothing_secret"]),
 ("events","crates/kai-core/src/engine/effect.rs","        self.same && !self.events.is_some_and(|n| n > 0)","        self.same",["--test","verify_e2e","events_confirm"]),
]
for name,f,a,b,args in M:
    src=open(f).read()
    assert a in src,(name,a)
    open(f,'w').write(src.replace(a,b,1))
    try:
        r=subprocess.run(["timeout","300","cargo","test","-q","-p","kai-core"]+args,capture_output=True,text=True)
        out=r.stdout+r.stderr
        res = "FAILS (guard holds)" if r.returncode!=0 else "still passes (!!)"
        if "error[" in out: res="COMPILE ERROR"
        print(f"{name:32} -> {res}", flush=True)
    finally:
        open(f,'w').write(src)
