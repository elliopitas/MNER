use std::collections::HashMap;
use std::fs;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub hosts: Vec<String>,
    pub workdir: String,
    pub executable: String,
    pub repeat: usize,
    pub arguments: HashMap<String, Vec<String>>,
}

#[derive(Debug)]
pub struct Permutation {
    pub id: String,
    pub parameters: String
}

impl Config {
    pub fn get_arguments_permutations(&self) -> Vec<Permutation> {
        let mut permutations: Vec<Permutation> = Vec::new();

        if self.arguments.is_empty() {
            return permutations;
        }

        // Convert HashMap to Vec for stable ordering
        let args: Vec<(String, Vec<String>)> = self.arguments.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Generate all permutations recursively
        generate_recursive(&args, 0, &mut vec![], &mut permutations);

        permutations
    }
}

fn generate_recursive(
    args: &[(String, Vec<String>)],
    index: usize,
    current: &mut Vec<(String, String)>,
    permutations: &mut Vec<Permutation>
) {
    if index == args.len() {
        // Build id and parameters from current combination
        let id = current.iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<_>>()
            .join("-");

        let parameters = current.iter()
            .map(|(key, value)| format!("--{}={}", key, value))
            .collect::<Vec<_>>()
            .join(" ");

        permutations.push(Permutation {
            id,
            parameters,
        });
        return;
    }

    let (key, values) = &args[index];
    for value in values {
        current.push((key.clone(), value.clone()));
        generate_recursive(args, index + 1, current, permutations);
        current.pop();
    }
}

pub fn parse_config(name: &str) -> Config {
    let s = fs::read_to_string(name).expect("failed to read config file");

    toml::from_str(&s).expect("failed to parse TOML into Config")
}
