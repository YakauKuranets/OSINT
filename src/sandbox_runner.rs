use std::process::Command;
use rand::Rng;

pub fn execute_ephemeral(url: &str, method: &str, headers: &[(&str, &str)]) -> Option<String> {
    let mut cmd = format!("curl -s -X {} '{}'", method, url);
    for (k, v) in headers {
        cmd.push_str(&format!(" -H '{}: {}'", k, v));
    }

    let container_name = format!("xgen_blackout_{}", rand::thread_rng().gen::<u64>());
    let output = Command::new("docker")
        .args([
            "run", "--rm",
            "--name", &container_name,
            "alpine/curl:latest",
            "sh", "-c", &cmd,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}