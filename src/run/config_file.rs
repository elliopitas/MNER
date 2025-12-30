use std::collections::HashMap;
use std::fs;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub name: String,
    pub hosts: Vec<String>,
    pub workdir: String,
    pub executable: String,
    pub repeat: usize,
    pub threads_per_task: usize,
    pub arguments: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Permutation {
    pub id: String,
    pub parameters: String
}

impl Config {
    pub fn new(name: &str) -> Config {
        let s = fs::read_to_string(name).expect("failed to read config file");

        toml::from_str(&s).expect("failed to parse TOML into Config")
    }
    pub fn get_arguments_permutations(&self) -> HashMap<String, String> {
        if self.arguments.is_empty() {
            return HashMap::new();
        }

        let combinations: usize = self.arguments.values().map(|v| v.len()).product();
        let mut permutations = HashMap::with_capacity(combinations * self.repeat);

        // Convert HashMap to Vec for stable ordering
        let mut args: Vec<(String, Vec<String>)> = self.arguments.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        args.sort_by(|a, b| a.0.cmp(&b.0));

        // Generate all permutations recursively
        generate_recursive(&args, 0, &mut vec![], &mut permutations, self.repeat);

        permutations
    }
}

fn generate_recursive(
    args: &[(String, Vec<String>)],
    index: usize,
    current: &mut Vec<(String, String)>,
    permutations: &mut HashMap<String, String>,
    repeat: usize,
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

        for i in 0..repeat {
            let id_with_repeat = format!("{}_{}", id, i);
            permutations.insert(id_with_repeat, parameters.clone());
        }
        return;
    }

    let (key, values) = &args[index];
    for value in values {
        current.push((key.clone(), value.clone()));
        generate_recursive(args, index + 1, current, permutations, repeat);
        current.pop();
    }
}
