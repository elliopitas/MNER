use async_ssh2_tokio::client::{Client, AuthMethod, ServerCheckMethod};
use std::env;
use tokio::process::Command;
use ssh_config::SSHConfig;
use std::fs;
use home;
use self_cell::self_cell;
use anyhow::{Context, Result};

self_cell!(
	struct SshConfigCell{
		owner: String,

		#[covariant]
		dependent: SSHConfig,
	}
);
pub struct NodeCommon {
	username: String,
	ssh_config: Option<SshConfigCell>
}

fn get_ssh_config_cell() -> Option<SshConfigCell> {
    let mut ssh_config_path = home::home_dir()?;
    ssh_config_path.push(".ssh/config");
    let config_string = fs::read_to_string(ssh_config_path).ok()?;
	SshConfigCell::try_new(config_string, |s| SSHConfig::parse_str(s)).ok()
}

impl NodeCommon {
	pub fn new() -> Self{
		Self{
			username: env::var("USER").unwrap_or_else(|_| "root".to_string()),
			ssh_config: get_ssh_config_cell()
		}
	}
}

pub struct Node {
	// common: &'a NodeCommon,
	pub hostname: String,
	pub client: Client,
	pub threads: usize,
}

impl Node {
	pub async fn try_new(common: & NodeCommon, hostname: &str) -> Result<Self> {
		let mut host_name = hostname.to_string();
		let mut port = 22;
		let mut user = common.username.as_str();

		if let Some(ssh_config_cell) = &common.ssh_config {
			ssh_config_cell.with_dependent(|_owner, ssh_config| {
				let params = ssh_config.query(hostname);
				if (!params.is_empty()) {
					if let Some(host_name_cfg) = params.get("HostName"){
						host_name = host_name_cfg.to_string();
					}
					if let Some(port_cfg) = params.get("Port"){
						port = port_cfg.parse::<u16>().unwrap_or(22);
					}
					if let Some(username_cfg) = params.get("User"){
						user = username_cfg;
					}
				}
			});
		}
		let client = Client::connect(
			(host_name, port),
			&user,
			AuthMethod::Agent,
			ServerCheckMethod::NoCheck,
		)
			.await
			.context("Failed to connect to host")?;

		let nproc_output = client.execute("nproc").await.context("failed to query for threads")?.stdout;
		let threads = nproc_output.trim().parse::<usize>().expect(format!("Failed to parse threads: {}", nproc_output.trim()).as_str());

		let node = Self {
			// common,
			hostname: hostname.to_string(),
			client,
			threads};
		Ok(node)
	}

	pub async fn rsync(&self, from: &str, to: &str) -> Result<()> {
		let mkdir_output = self.client.execute(format!("mkdir -p {}", to).as_str()).await.context("Failed to create remote directory")?;
		if mkdir_output.exit_status != 0 {
			return Err(anyhow::anyhow!("Failed to create remote directory: {}", mkdir_output.stderr));
		}
		let from_path = if from.ends_with('/') && !fs::metadata(from).map(|m| m.is_dir()).unwrap_or(false){
			from.to_string()
		}else {
			format!("{}/", from)
		};
		let to = format!("{}:{}", self.hostname, to);
		let output = Command::new("rsync").arg("-arz").arg(from_path).arg(to).output().await?;
		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			return Err(anyhow::anyhow!("rsync failed: {}", stderr));
		}
		Ok(())
	}

	pub async fn rm(&self, dir: &str) -> Result<()> {
		let output = self.client.execute(format!("rm -rf {dir}").as_str()).await.context("Failed to execute rm")?;
		if output.exit_status != 0 {
			return Err(anyhow::anyhow!("rm failed: {}", output.stderr));
		}

		Ok(())
	}

	// pub async fn cd(&self, dir: &str) -> Result<()> {
	// 	let output = self.client.execute(format!("cd {dir}").as_str()).await.context("Failed to execute cd")?;
	// 	if output.exit_status != 0 {
	// 		return Err(anyhow::anyhow!("cd failed: {}", output.stderr));
	// 	}
	//
	// 	Ok(())
	// }



}
