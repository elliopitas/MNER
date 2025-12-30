use crate::run::node::{Node, NodeCommon};
use futures::future::{try_join_all};
use futures::TryFutureExt;
use anyhow::{Result};


// #[derive(Debug)]
// pub enum Error {
// 	Ssh(async_ssh2_tokio::Error),
// 	NodeCreation {
// 		hostname: String,
// 		source: anyhow::Error,
// 	},
// }

pub struct Nodes {
	common: NodeCommon,
	pub nodes: Vec<Node>,
}

impl Nodes {
	pub async fn new(nodes_hostnames: &Vec<String>) -> Result<Self> {
		let mut nodes = Nodes{
			common: NodeCommon::new(),
			nodes : Vec::with_capacity(nodes_hostnames.len()),
		};
		let common_ref = &nodes.common;
		let node_futures = nodes_hostnames.iter().map(move |hostname| {
			Node::try_new(common_ref, hostname)
				.map_err(move |e| {
					e.context(format!("Failed to create node for hostname '{}'", hostname))
				})
				// .map_err(move |e| Error::NodeCreation {
				// 	hostname: hostname.to_string(),
				// 	source: e,
				// })

		});

		nodes.nodes.extend(try_join_all(node_futures).await?);
		Ok(nodes)
	}
}