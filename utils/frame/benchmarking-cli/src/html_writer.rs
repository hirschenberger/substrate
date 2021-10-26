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
//
// Outputs benchmark results to a HTML file with details, summaries and charts

use std::{convert::TryInto, fs, io::Read, path::PathBuf};
use crate::{BenchmarkCmd, utils::{self, CmdData}};
use frame_benchmarking::{
	Analysis, AnalysisChoice, BenchmarkBatchSplitResults, BenchmarkResult, BenchmarkSelector,
	RegressionModel,
};
use frame_support::traits::StorageInfo;
use serde::Serialize;
use inflector::Inflector;
const VERSION: &'static str = env!("CARGO_PKG_VERSION");
//const TEMPLATE: &str = include_str!("./html_template.hbs");

// This is the final structure we will pass to the Handlebars template.
#[derive(Serialize, Default, Debug, Clone)]
struct TemplateData {
	args: Vec<String>,
	date: String,
	version: String,
	pallet: String,
	instance: String,
	cmd: utils::CmdData,
	benchmarks: Vec<utils::BenchmarkData>,
}

pub fn write_results(
	batches: &[BenchmarkBatchSplitResults],
	storage_info: &[StorageInfo],
	path: &PathBuf,
	cmd: &BenchmarkCmd,
) -> Result<(), std::io::Error> {
	let mut file = fs::File::open("./html_template.hbs")?;
	let mut TEMPLATE = String::new();
	file.read_to_string(&mut TEMPLATE)?;

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

	let analysis_choice = cmd.analysis_choice()?;

	// Organize results by pallet into a JSON map
	let all_results = utils::map_results(batches, &storage_info, &analysis_choice)?;
	for ((pallet, instance), results) in all_results.iter() {
		let mut file_path = path.clone();
		// If a user only specified a directory...
		if file_path.is_dir() {
			// Check if there might be multiple instances benchmarked.
			if all_results.keys().any(|(p, i)| p == pallet && i != instance) {
				// Create new file: "path/to/pallet_name_instance_name.html".
				file_path.push(pallet.clone() + "_" + &instance.to_snake_case());
			} else {
				// Create new file: "path/to/pallet_name.html".
				file_path.push(pallet.clone());
			}
			file_path.set_extension("html");
		}

		let hbs_data = TemplateData {
			args: args.clone(),
			date: date.clone(),
			version: VERSION.to_string(),
			pallet: pallet.to_string(),
			instance: instance.to_string(),
			cmd: cmd_data.clone(),
			benchmarks: results.clone(),
		};

		let mut output_file = fs::File::create(file_path)?;
		handlebars
			.render_template_to_write(&TEMPLATE, &hbs_data, &mut output_file)
			.map_err(|e| utils::io_error(&e.to_string()))?;
	}

	Ok(())
}
