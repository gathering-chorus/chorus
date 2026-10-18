// #4590 — record which source tree this binary was built from, so a land can
// refuse a build of some other card's code. The #4576 land ran the role slot's
// athena-deploy, rebuilt an hour earlier by another card: none of #4576's deploy
// code was in it, and prod lost its session shapes.
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git").args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../shared");
    let tree = (|| {
        let top = git(&["rev-parse", "--show-toplevel"])?;
        let a = git(&["-C", &top, "rev-parse", "HEAD:platform/services/athena-deploy/src"])?;
        let s = git(&["-C", &top, "rev-parse", "HEAD:platform/services/shared"])?;
        let dirty = git(&["-C", &top, "status", "--porcelain", "--", "platform/services/athena-deploy/src", "platform/services/shared"])
            .map(|o| !o.is_empty()).unwrap_or(true);
        Some(format!("{a}+{s}{}", if dirty { "+dirty" } else { "" }))
    })().unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=ATHENA_DEPLOY_SRC_TREE={tree}");
}
