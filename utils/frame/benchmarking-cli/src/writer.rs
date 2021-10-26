// This file is part of Substrate.

// Copyright (C) 2020-2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Outputs benchmark results to Rust files that can be ingested by the runtime.

use std::{convert::TryInto, fs, path::PathBuf};

use frame_benchmarking::{AnalysisChoice, BenchmarkBatchSplitResults};
use serde::Serialize;

use crate::{BenchmarkCmd, utils::{self, CmdData}};
use frame_support::traits::StorageInfo;
use inflector::Inflector;


const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const TEMPLATE: &str = include_str!("./template.hbs");

// This is the final structure we will pass to the Handlebars template.
#[derive(Serialize, Default, Debug, Clone)]
struct TemplateData {
	args: Vec<String>,
	date: String,
	version: String,
	pallet: String,
	instance: String,
	header: String,
	cmd: utils::CmdData,
	benchmarks: Vec<utils::BenchmarkData>,
}

// Create weight file from benchmark data and Handlebars template.
pub fn write_results(
	batches: &[BenchmarkBatchSplitResults],
	storage_info: &[StorageInfo],
	path: &PathBuf,
	cmd: &BenchmarkCmd,
) -> Result<(), std::io::Error> {
	// Use custom template if provided.
	let template: String = match &cmd.template {
		Some(template_file) => fs::read_to_string(template_file)?,
		None => TEMPLATE.to_string(),
	};

	// Use header if provided
	let header_text = match &cmd.header {
		Some(header_file) => {
			let text = fs::read_to_string(header_file)?;
			text
		},
		None => String::new(),
	};

	// Date string metadata
	let date = chrono::Utc::now().format("%Y-%m-%d").to_string();

	// Full CLI args passed to trigger the benchmark.
	let args = std::env::args().collect::<Vec<String>>();

	// Which analysis function should be used when outputting benchmarks
	let cmd_data: CmdData = cmd.try_into()?;

	// New Handlebars instance with helpers.
	let mut handlebars = handlebars::Handlebars::new();
	handlebars.register_helper("underscore", Box::new(utils::UnderscoreHelper));
	handlebars.register_helper("join", Box::new(utils::JoinHelper));
	// Don't HTML escape any characters.
	handlebars.register_escape_fn(|s| -> String { s.to_string() });

	let analysis_choice = cmd.analysis_choice()?;

	// Organize results by pallet into a JSON map
	let all_results = utils::map_results(batches, &storage_info, &analysis_choice)?;
	for ((pallet, instance), results) in all_results.iter() {
		let mut file_path = path.clone();
		// If a user only specified a directory...
		if file_path.is_dir() {
			// Check if there might be multiple instances benchmarked.
			if all_results.keys().any(|(p, i)| p == pallet && i != instance) {
				// Create new file: "path/to/pallet_name_instance_name.rs".
				file_path.push(pallet.clone() + "_" + &instance.to_snake_case());
			} else {
				// Create new file: "path/to/pallet_name.rs".
				file_path.push(pallet.clone());
			}
			file_path.set_extension("rs");
		}

		let hbs_data = TemplateData {
			args: args.clone(),
			date: date.clone(),
			version: VERSION.to_string(),
			pallet: pallet.to_string(),
			instance: instance.to_string(),
			header: header_text.clone(),
			cmd: cmd_data.clone(),
			benchmarks: results.clone(),
		};

		let mut output_file = fs::File::create(file_path)?;
		handlebars
			.render_template_to_write(&template, &hbs_data, &mut output_file)
			.map_err(|e| utils::io_error(&e.to_string()))?;
	}
	Ok(())
}


#[cfg(test)]
mod test {
	use super::*;
	use frame_benchmarking::{BenchmarkBatchSplitResults, BenchmarkParameter, BenchmarkResult};

	fn test_data(
		pallet: &[u8],
		benchmark: &[u8],
		param: BenchmarkParameter,
		base: u32,
		slope: u32,
	) -> BenchmarkBatchSplitResults {
		let mut results = Vec::new();
		for i in 0..5 {
			results.push(BenchmarkResult {
				components: vec![(param, i), (BenchmarkParameter::z, 0)],
				extrinsic_time: (base + slope * i).into(),
				storage_root_time: (base + slope * i).into(),
				reads: (base + slope * i).into(),
				repeat_reads: 0,
				writes: (base + slope * i).into(),
				repeat_writes: 0,
				proof_size: 0,
				keys: vec![],
			})
		}

		return BenchmarkBatchSplitResults {
			pallet: [pallet.to_vec(), b"_pallet".to_vec()].concat(),
			instance: b"instance".to_vec(),
			benchmark: [benchmark.to_vec(), b"_benchmark".to_vec()].concat(),
			time_results: results.clone(),
			db_results: results,
		}
	}

	fn check_data(benchmark: &BenchmarkData, component: &str, base: u128, slope: u128) {
		assert_eq!(
			benchmark.components,
			vec![
				Component { name: component.to_string(), is_used: true },
				Component { name: "z".to_string(), is_used: false },
			],
		);
		// Weights multiplied by 1,000
		assert_eq!(benchmark.base_weight, base * 1_000);
		assert_eq!(
			benchmark.component_weight,
			vec![ComponentSlope { name: component.to_string(), slope: slope * 1_000, error: 0 }]
		);
		// DB Reads/Writes are untouched
		assert_eq!(benchmark.base_reads, base);
		assert_eq!(
			benchmark.component_reads,
			vec![ComponentSlope { name: component.to_string(), slope, error: 0 }]
		);
		assert_eq!(benchmark.base_writes, base);
		assert_eq!(
			benchmark.component_writes,
			vec![ComponentSlope { name: component.to_string(), slope, error: 0 }]
		);
	}

	#[test]
	fn map_results_works() {
		let mapped_results = map_results(
			&[
				test_data(b"first", b"first", BenchmarkParameter::a, 10, 3),
				test_data(b"first", b"second", BenchmarkParameter::b, 9, 2),
				test_data(b"second", b"first", BenchmarkParameter::c, 3, 4),
			],
			&[],
			&AnalysisChoice::default(),
		)
		.unwrap();

		let first_benchmark = &mapped_results
			.get(&("first_pallet".to_string(), "instance".to_string()))
			.unwrap()[0];
		assert_eq!(first_benchmark.name, "first_benchmark");
		check_data(first_benchmark, "a", 10, 3);

		let second_benchmark = &mapped_results
			.get(&("first_pallet".to_string(), "instance".to_string()))
			.unwrap()[1];
		assert_eq!(second_benchmark.name, "second_benchmark");
		check_data(second_benchmark, "b", 9, 2);

		let second_pallet_benchmark = &mapped_results
			.get(&("second_pallet".to_string(), "instance".to_string()))
			.unwrap()[0];
		assert_eq!(second_pallet_benchmark.name, "first_benchmark");
		check_data(second_pallet_benchmark, "c", 3, 4);
	}
}
