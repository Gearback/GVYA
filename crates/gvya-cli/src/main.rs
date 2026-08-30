//! Canonical GVYA source CLI. It calls the Rust compiler/runtime directly; it does not reimplement semantics.

mod authoring_check;
mod authoring_loop;

mod commands;
mod diagnostics;
mod io_support;
mod package_check;
mod reports;
mod runtime_driver;
mod scaffold;
mod source_io;
#[cfg(test)]
mod tests;

use authoring_loop::*;
use commands::*;
use diagnostics::*;
use io_support::*;
use package_check::*;
use reports::*;
use runtime_driver::*;
use scaffold::*;
use source_io::*;

use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use authoring_check::{AuthoringAcceptancePolicy, evaluate};

use gvya_compiler::{
    analysis::{AnalysisLimits, ProjectAnalysis, analyze_project},
    audit::{AuditReport, Auditor, AuditorLimits},
    authoring::{AuthoringAction, AuthoringLoopDecision, plan_authoring_step},
    change::{ChangePlanLimits, ChangeTestPlan, ProjectSourceSurface, plan_change_tests},
    package::{ComposedProject, compose_packages},
    pipeline::{
        BuildOptions, SignatureEnvelope, artifact_signing_content_root, attach_signature_envelope,
        build_source_project,
    },
    source::{
        SourceLimits, SourceTree, contract::source_contract_json,
        inventory::source_object_inventory_json, resolve_source_project, safe_source_path,
    },
    testing::{
        SimulationDriver, SimulationInteractionInput, SimulationObservation,
        SimulationProposalReceipt, TestRunLimits, run_test_suite,
    },
};
use gvya_kernel::conversation::{language_tag_is_well_formed, normalize_locale};
use gvya_model::{
    AdmissionOutcome, CapabilityId, ConfirmationHint, EffectClass, ResponseItem, Value,
};
use gvya_runtime::{
    LoadPolicy, Runtime, RuntimeCapabilityResultRequest, RuntimeLimits, RuntimeOpenRequest,
    RuntimeTurnRequest, RuntimeUtteranceInput,
    wire::{
        parse_capability_result_request, parse_turn_request, serialize_capability_result_result,
        serialize_turn_result,
    },
};

const MACHINE_JSON_MAX_BYTES: usize = 16 * 1024 * 1024;
const AUTHORING_POLICY_MAX_BYTES: usize = 64 * 1024;
const SIGNATURE_ENVELOPE_MAX_BYTES: usize = 32 * 1024;

const USAGE: &str = "GVYA canonical CLI\n\n  gvya init bot OUTPUT_DIR [OPTIONS]\n  gvya init package OUTPUT_DIR [OPTIONS]\n  gvya check-package PACKAGE [--policy POLICY.json]\n  gvya check [PROJECT] [--policy POLICY.json]\n  gvya check-change BASE_PROJECT CANDIDATE_PROJECT [--json]\n  gvya author-step BASE_PROJECT CANDIDATE_PROJECT --json\n  gvya build [PROJECT] --output FILE.gvya\n  gvya schema [--kind KIND] --json\n  gvya inspect [PROJECT] [--kind KIND [--id ID]] --json\n  gvya capabilities [PROJECT] --json\n  gvya capability [PROJECT] --id CAPABILITY --json\n  gvya analysis [PROJECT] --json\n  gvya audit [PROJECT] [--json]\n  gvya test [PROJECT] [--json]\n  gvya turn [PROJECT] --request REQUEST.json\n  gvya capability-result [PROJECT] --request REQUEST.json\n  gvya signing-root ARTIFACT.gvya\n  gvya attach-signature ARTIFACT.gvya --envelope FILE.json --output SIGNED.gvya\n\nPROJECT is gvya.project.json or its containing directory. PACKAGE is package.json or its containing directory. init/check commands emit bounded structured JSON by default. Signing remains external.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("gvya: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = args.first().map(String::as_str) else {
        return Err(USAGE.into());
    };
    match command {
        "init" => command_init(&args[1..]),
        "check-package" => command_check_package(&args[1..]),
        "check" => command_check(&args[1..]),
        "check-change" => command_check_change(&args[1..]),
        "author-step" => command_author_step(&args[1..]),
        "build" => command_build(&args[1..]),
        "schema" => command_schema(&args[1..]),
        "inspect" => command_inspect(&args[1..]),
        "capabilities" => command_capabilities(&args[1..]),
        "capability" => command_capability(&args[1..]),
        "analysis" => command_analysis(&args[1..]),
        "audit" => command_audit(&args[1..]),
        "test" => command_test(&args[1..]),
        "turn" => command_turn(&args[1..]),
        "capability-result" => command_capability_result(&args[1..]),
        "signing-root" => command_signing_root(&args[1..]),
        "attach-signature" => command_attach_signature(&args[1..]),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(format!("unknown command {command:?}\n\n{USAGE}")),
    }
}
