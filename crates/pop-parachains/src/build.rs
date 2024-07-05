// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use anyhow::Result;
use duct::cmd;
use pop_common::{manifest::from_path, replace_in_file, Profile};
use serde_json::Value;
use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
};

/// Build the parachain and returns the path to the binary.
///
/// # Arguments
/// * `path` - The optional path to the parachain manifest, defaulting to the current directory if not specified.
/// * `package` - The optional package to be built.
/// * `release` - Whether the parachain should be built without any debugging functionality.
/// * `node_path` - An optional path to the node directory. Defaults to the `node` subdirectory of the project path if not provided.
pub fn build_parachain(
	path: &Path,
	package: Option<String>,
	profile: &Profile,
	node_path: Option<&Path>,
) -> Result<PathBuf, Error> {
	let mut args = vec!["build"];
	if let Some(package) = package.as_deref() {
		args.push("--package");
		args.push(package)
	}
	if matches!(profile, &Profile::Release) {
		args.push("--release");
	}
	cmd("cargo", args).dir(path).run()?;
	binary_path(&profile.target_folder(path), node_path.unwrap_or(&path.join("node")))
}

/// Determines whether the manifest at the supplied path is a supported parachain project.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not specified.
pub fn is_supported(path: Option<&Path>) -> Result<bool, Error> {
	let manifest = pop_common::manifest::from_path(path)?;
	// Simply check for a parachain dependency
	const DEPENDENCIES: [&str; 4] =
		["cumulus-client-collator", "cumulus-primitives-core", "parachains-common", "polkadot-sdk"];
	Ok(DEPENDENCIES.into_iter().any(|d| {
		manifest.dependencies.contains_key(d)
			|| manifest.workspace.as_ref().map_or(false, |w| w.dependencies.contains_key(d))
	}))
}

/// Constructs the node binary path based on the target path and the node folder path.
///
/// # Arguments
/// * `target_path` - The path where the binaries are expected to be found.
/// * `node_path` - The path to the node from which the node name will be parsed.
fn binary_path(target_path: &Path, node_path: &Path) -> Result<PathBuf, Error> {
	let manifest = from_path(Some(node_path))?;
	let node_name = manifest.package().name();
	let release = target_path.join(node_name);
	if !release.exists() {
		return Err(Error::MissingBinary(node_name.to_string()));
	}
	Ok(release)
}

/// Generates the plain text chain specification for a parachain.
///
/// # Arguments
/// * `binary_path` - The path to the node binary executable that contains the `build-spec` command.
/// * `plain_chain_spec` - Location of the plain_parachain_spec file to be generated.
/// * `para_id` - The parachain ID to be replaced in the specification.
pub fn generate_plain_chain_spec(
	binary_path: &Path,
	plain_chain_spec: &Path,
	para_id: u32,
) -> Result<(), Error> {
	check_command_exists(&binary_path, "build-spec")?;
	cmd(binary_path, vec!["build-spec", "--disable-default-bootnode"])
		.stdout_path(plain_chain_spec)
		.run()?;
	let generated_para_id = get_parachain_id(plain_chain_spec)?.unwrap_or(para_id.into()) as u32;
	replace_para_id(plain_chain_spec.to_path_buf(), para_id, generated_para_id)?;
	Ok(())
}

/// Generates a raw chain specification file for a parachain.
///
/// # Arguments
/// * `binary_path` - The path to the node binary executable that contains the `build-spec` command.
/// * `plain_chain_spec` - Location of the plain chain specification file.
/// * `chain_spec_file_name` - The name of the chain specification file to be generated.
pub fn generate_raw_chain_spec(
	binary_path: &Path,
	plain_chain_spec: &Path,
	chain_spec_file_name: &str,
) -> Result<PathBuf, Error> {
	if !plain_chain_spec.exists() {
		return Err(Error::MissingChainSpec(plain_chain_spec.display().to_string()));
	}
	check_command_exists(&binary_path, "build-spec")?;
	let raw_chain_spec =
		plain_chain_spec.parent().unwrap_or(Path::new("./")).join(chain_spec_file_name);
	cmd(
		binary_path,
		vec![
			"build-spec",
			"--chain",
			&plain_chain_spec.display().to_string(),
			"--disable-default-bootnode",
			"--raw",
		],
	)
	.stdout_path(&raw_chain_spec)
	.run()?;
	Ok(raw_chain_spec)
}

/// Export the WebAssembly runtime for the parachain.
///
/// # Arguments
/// * `binary_path` - The path to the node binary executable that contains the `export-genesis-wasm` command.
/// * `chain_spec` - Location of the raw chain specification file.
/// * `wasm_file_name` - The name of the wasm runtime file to be generated.
pub fn export_wasm_file(
	binary_path: &Path,
	chain_spec: &Path,
	wasm_file_name: &str,
) -> Result<PathBuf, Error> {
	if !chain_spec.exists() {
		return Err(Error::MissingChainSpec(chain_spec.display().to_string()));
	}
	check_command_exists(&binary_path, "export-genesis-wasm")?;
	let wasm_file = chain_spec.parent().unwrap_or(Path::new("./")).join(wasm_file_name);
	cmd(
		binary_path,
		vec![
			"export-genesis-wasm",
			"--chain",
			&chain_spec.display().to_string(),
			&wasm_file.display().to_string(),
		],
	)
	.run()?;
	Ok(wasm_file)
}

/// Generate the parachain genesis state.
///
/// # Arguments
/// * `binary_path` - The path to the node binary executable that contains the `export-genesis-state` command.
/// * `chain_spec` - Location of the raw chain specification file.
/// * `genesis_file_name` - The name of the genesis state file to be generated.
pub fn generate_genesis_state_file(
	binary_path: &Path,
	chain_spec: &Path,
	genesis_file_name: &str,
) -> Result<PathBuf, Error> {
	if !chain_spec.exists() {
		return Err(Error::MissingChainSpec(chain_spec.display().to_string()));
	}
	check_command_exists(&binary_path, "export-genesis-state")?;
	let genesis_file = chain_spec.parent().unwrap_or(Path::new("./")).join(genesis_file_name);
	cmd(
		binary_path,
		vec![
			"export-genesis-state",
			"--chain",
			&chain_spec.display().to_string(),
			&genesis_file.display().to_string(),
		],
	)
	.run()?;
	Ok(genesis_file)
}

/// Get the parachain id from the chain specification file.
fn get_parachain_id(chain_spec: &Path) -> Result<Option<u64>> {
	let data = fs::read_to_string(chain_spec)?;
	let value = serde_json::from_str::<Value>(&data)?;
	Ok(value.get("para_id").and_then(Value::as_u64))
}

/// Replaces the generated parachain id in the chain specification file with the provided para_id.
fn replace_para_id(chain_spec: PathBuf, para_id: u32, generated_para_id: u32) -> Result<()> {
	let mut replacements_in_cargo: HashMap<&str, &str> = HashMap::new();
	let old_para_id = format!("\"para_id\": {generated_para_id}");
	let new_para_id = format!("\"para_id\": {para_id}");
	replacements_in_cargo.insert(&old_para_id, &new_para_id);
	let old_parachain_id = format!("\"parachainId\": {generated_para_id}");
	let new_parachain_id = format!("\"parachainId\": {para_id}");
	replacements_in_cargo.insert(&old_parachain_id, &new_parachain_id);
	replace_in_file(chain_spec, replacements_in_cargo)?;
	Ok(())
}

/// Checks if a given command exists and can be executed by running it with the "--help" argument.
fn check_command_exists(binary_path: &Path, command: &str) -> Result<(), Error> {
	cmd(binary_path, vec![command, "--help"]).stdout_null().run().map_err(|_err| {
		Error::MissingCommand {
			command: command.to_string(),
			binary: binary_path.display().to_string(),
		}
	})?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		new_parachain::instantiate_standard_template, templates::Parachain, Config, Zombienet,
	};
	use anyhow::Result;
	use pop_common::manifest::{self, Dependency};
	use std::{
		fs,
		fs::{metadata, write},
		io::Write,
		os::unix::fs::PermissionsExt,
		path::Path,
	};
	use tempfile::{tempdir, Builder};

	fn setup_template_and_instantiate() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		instantiate_standard_template(&Parachain::Standard, temp_dir.path(), config, None)?;
		Ok(temp_dir)
	}

	// Function that mocks the build process generating the target dir and release.
	fn mock_build_process(temp_dir: &Path) -> Result<(), Error> {
		// Create a target directory
		let target_dir = temp_dir.join("target");
		fs::create_dir(&target_dir)?;
		fs::create_dir(&target_dir.join("release"))?;
		// Create a release file
		fs::File::create(target_dir.join("release/parachain-template-node"))?;
		Ok(())
	}

	// Function that generates a Cargo.toml inside node folder for testing.
	fn generate_mock_node(temp_dir: &Path) -> Result<(), Error> {
		// Create a node directory
		let target_dir = temp_dir.join("node");
		fs::create_dir(&target_dir)?;
		// Create a Cargo.toml file
		let mut toml_file = fs::File::create(target_dir.join("Cargo.toml"))?;
		writeln!(
			toml_file,
			r#"
			[package]
			name = "parachain_template_node"
			version = "0.1.0"

			[dependencies]

			"#
		)?;
		Ok(())
	}

	// Function that fetch a binary from pop network
	async fn fetch_binary(cache: &Path) -> Result<String, Error> {
		let config = Builder::new().suffix(".toml").tempfile()?;
		writeln!(
			config.as_file(),
			r#"
[relaychain]
chain = "rococo-local"

[[parachains]]
id = 4385
default_command = "pop-node"
"#
		)?;
		let mut zombienet =
			Zombienet::new(&cache, config.path().to_str().unwrap(), None, None, None, None, None)
				.await?;
		let mut binary_name: String = "".to_string();
		for binary in zombienet.binaries().filter(|b| !b.exists() && b.name() == "pop-node") {
			binary_name = format!("{}-{}", binary.name(), binary.latest().unwrap());
			binary.source(true, &(), true).await?;
		}
		Ok(binary_name)
	}

	// Replace the binary fetched with the mocked binary
	fn replace_mock_with_binary(temp_dir: &Path, binary_name: String) -> Result<PathBuf, Error> {
		let binary_path = temp_dir.join(binary_name);
		let content = fs::read(&binary_path)?;
		fs::write(temp_dir.join("target/release/parachain-template-node"), content)?;
		// Make executable
		let mut perms =
			metadata(temp_dir.join("target/release/parachain-template-node"))?.permissions();
		perms.set_mode(0o755);
		std::fs::set_permissions(temp_dir.join("target/release/parachain-template-node"), perms)?;
		Ok(binary_path)
	}

	#[test]
	fn build_parachain_works() -> Result<()> {
		let temp_dir = tempdir()?;
		let name = "parachain_template_node";
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		generate_mock_node(&temp_dir.path().join(name))?;
		let binary = build_parachain(&temp_dir.path().join(name), None, &Profile::Release, None)?;
		let target_folder = temp_dir.path().join(name).join("target/release");
		assert!(target_folder.exists());
		assert!(target_folder.join("parachain_template_node").exists());
		assert_eq!(
			binary.display().to_string(),
			target_folder.join("parachain_template_node").display().to_string()
		);
		Ok(())
	}

	#[test]
	fn binary_path_works() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		mock_build_process(temp_dir.path())?;
		let release_path =
			binary_path(&temp_dir.path().join("target/release"), &temp_dir.path().join("node"))?;
		assert_eq!(
			release_path.display().to_string(),
			format!("{}/target/release/parachain-template-node", temp_dir.path().display())
		);
		Ok(())
	}

	#[test]
	fn binary_path_fails_missing_binary() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		assert!(matches!(
			binary_path(&temp_dir.path().join("target/release"), &temp_dir.path().join("node")),
			Err(Error::MissingBinary(error)) if error == "parachain-template-node"
		));
		Ok(())
	}

	#[tokio::test]
	async fn generate_files_works() -> anyhow::Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		mock_build_process(temp_dir.path())?;
		let binary_name = fetch_binary(temp_dir.path()).await?;
		let binary_path = replace_mock_with_binary(temp_dir.path(), binary_name)?;
		// Test generate chain spec
		let plain_chain_spec = &temp_dir.path().join("plain-parachain-chainspec.json");
		generate_plain_chain_spec(
			&binary_path,
			&temp_dir.path().join("plain-parachain-chainspec.json"),
			2001,
		)?;
		assert!(plain_chain_spec.exists());
		let raw_chain_spec = generate_raw_chain_spec(
			&binary_path,
			&plain_chain_spec,
			"raw-parachain-chainspec.json",
		)?;
		assert!(raw_chain_spec.exists());
		let content = fs::read_to_string(raw_chain_spec.clone()).expect("Could not read file");
		assert!(content.contains("\"para_id\": 2001"));
		// Test export wasm file
		let wasm_file = export_wasm_file(&binary_path, &raw_chain_spec, "para-2001-wasm")?;
		assert!(wasm_file.exists());
		// Test generate parachain state file
		let genesis_file =
			generate_genesis_state_file(&binary_path, &raw_chain_spec, "para-2001-genesis-state")?;
		assert!(genesis_file.exists());
		Ok(())
	}

	#[test]
	fn raw_chain_spec_fails_wrong_chain_spec() -> Result<()> {
		assert!(matches!(
			generate_raw_chain_spec(
				Path::new("./binary"),
				Path::new("./plain-parachain-chainspec.json"),
				"plain-parachain-chainspec.json"
			),
			Err(Error::MissingChainSpec(..))
		));
		Ok(())
	}

	#[test]
	fn export_wasm_file_fails_wrong_chain_spec() -> Result<()> {
		assert!(matches!(
			export_wasm_file(
				Path::new("./binary"),
				Path::new("./raw-parachain-chainspec"),
				"para-2001-wasm"
			),
			Err(Error::MissingChainSpec(..))
		));
		Ok(())
	}

	#[test]
	fn generate_genesis_state_file_wrong_chain_spec() -> Result<()> {
		assert!(matches!(
			generate_genesis_state_file(
				Path::new("./binary"),
				Path::new("./raw-parachain-chainspec"),
				"para-2001-genesis-state",
			),
			Err(Error::MissingChainSpec(..))
		));
		Ok(())
	}

	#[test]
	fn get_parachain_id_works() -> Result<()> {
		let mut file = tempfile::NamedTempFile::new()?;
		writeln!(file, r#"{{ "name": "Local Testnet", "para_id": 2002 }}"#)?;
		let get_parachain_id = get_parachain_id(&file.path())?;
		assert_eq!(get_parachain_id, Some(2002));
		Ok(())
	}

	#[test]
	fn replace_para_id_works() -> Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let file_path = temp_dir.path().join("chain-spec.json");
		let mut file = fs::File::create(temp_dir.path().join("chain-spec.json"))?;
		writeln!(
			file,
			r#"
				"name": "Local Testnet",
				"para_id": 1000,
				"parachainInfo": {{
					"parachainId": 1000
				}},
			"#
		)?;
		replace_para_id(file_path.clone(), 2001, 1000)?;
		let content = fs::read_to_string(file_path).expect("Could not read file");
		assert_eq!(
			content.trim(),
			r#"
				"name": "Local Testnet",
				"para_id": 2001,
				"parachainInfo": {
					"parachainId": 2001
				},
			"#
			.trim()
		);
		Ok(())
	}

	#[test]
	fn check_command_exists_fails() -> Result<()> {
		let binary_path = PathBuf::from("/bin");
		let cmd = "nonexistent_command";
		assert!(matches!(
			check_command_exists(&binary_path, cmd),
			Err(Error::MissingCommand {command, binary })
			if command == cmd && binary == binary_path.display().to_string()
		));
		Ok(())
	}

	#[test]
	fn is_supported_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(&path).run()?;
		assert!(!is_supported(Some(&path.join(name)))?);

		// Parachain
		let mut manifest = manifest::from_path(Some(&path.join(name)))?;
		manifest
			.dependencies
			.insert("cumulus-client-collator".into(), Dependency::Simple("^0.14.0".into()));
		let manifest = toml_edit::ser::to_string_pretty(&manifest)?;
		write(path.join(name).join("Cargo.toml"), manifest)?;
		assert!(is_supported(Some(&path.join(name)))?);
		Ok(())
	}
}
