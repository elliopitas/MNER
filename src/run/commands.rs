use tokio::process::Command;

pub async fn rsync(from: &str, to: &str, delete_src: bool) -> anyhow::Result<()> {
	/*let from = if from.ends_with('/') && !fs::metadata(from).map(|m| m.is_dir()).unwrap_or(false){
		from.to_string()
	}else {
		format!("{}/", from)
	};*/
	let mut output_cmd = Command::new("rsync");
	output_cmd.arg("-arz").arg("--delete").arg("--mkpath");
	if delete_src {
		output_cmd.arg("--remove-source-files");
	}
	output_cmd.arg(from).arg(to);
	let output = output_cmd.output().await?;
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		return Err(anyhow::anyhow!("rsync failed: {}", stderr));
	}
	Ok(())
}