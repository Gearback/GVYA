from __future__ import print_function
import argparse
import json
import os
import tempfile
import time
from pathlib import Path

from common import extract_token, load_spec, score_rows, token_to_intent, write_json, write_jsonl


def package_version():
    # Program-Y 3.6's package __version__ is stale (0.9.0), so read the
    # distribution metadata that ships in the official 3.6 sdist. This also
    # avoids importing setuptools/pkg_resources on minimal Python 3.7 builds.
    import programy
    package_root = Path(programy.__file__).resolve().parent
    egg_info = package_root.parent / "programy.egg-info" / "PKG-INFO"
    if egg_info.is_file():
        for line in egg_info.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.startswith("Version: "):
                return line.split(":", 1)[1].strip()
    return "unknown"


def make_client(category_dir):
    """Create a Program-Y 3.6 client with only the frozen benchmark AIML as category knowledge."""
    import programy
    from programy.chatbot import ProgramYChatbot
    from programy.config.file.yaml_file import YamlConfigurationFile
    from programy.config.programy import ProgramyConfiguration

    package_root = Path(programy.__file__).resolve().parent
    pattern_nodes = package_root / "parser" / "pattern" / "pattern_nodes.conf"
    template_nodes = package_root / "parser" / "template" / "template_nodes.conf"
    if not pattern_nodes.is_file() or not template_nodes.is_file():
        raise RuntimeError("Program-Y 3.6 parser node configuration files are missing")

    # Program-Y 3.6 ships a minimal ProgramYChatbot using this same configuration path:
    # ConsoleBotClient -> ProgramyConfiguration -> YamlConfigurationFile -> real Brain/AIMLParser.
    # We override only the category directory so no packaged semantic AIML is loaded.
    class BenchmarkProgramYChatbot(ProgramYChatbot):
        def parse_arguments(self, argument_parser):
            # Embedded benchmark execution: do not let Program-Y parse the
            # benchmark runner's own CLI arguments. This mirrors Program-Y's
            # shipped MyEmbeddedBot client.
            from programy.clients.args import CommandLineClientArguments
            args = CommandLineClientArguments(self, parser=None)
            args._logging = None
            return args

        def load_configuration(self, arguments):
            client_config = self.get_client_configuration()
            self._configuration = ProgramyConfiguration(client_config)
            yaml_file = YamlConfigurationFile()
            yaml_file.load_from_text(
                """
console:
  description: GVYA benchmark Program-Y 3.6 client
  bot: bot
  prompt: ">>>"

  storage:
    entities:
      categories: file
      template_nodes: file
      pattern_nodes: file
    stores:
      file:
        type: file
        config:
          categories_storage:
            dirs: "{categories}"
            subdirs: false
            extension: .aiml
          pattern_nodes_storage:
            file: "{pattern_nodes}"
          template_nodes_storage:
            file: "{template_nodes}"

bot:
  brain: brain
  initial_question: BENCH:FALLBACK
  default_response: BENCH:FALLBACK
  exit_response: BENCH:FALLBACK
  override_properties: true
""".format(
                    categories=str(Path(category_dir).resolve()).replace("\\", "/"),
                    pattern_nodes=str(pattern_nodes).replace("\\", "/"),
                    template_nodes=str(template_nodes).replace("\\", "/"),
                ),
                client_config,
                ".",
            )

    return BenchmarkProgramYChatbot(argument_parser=None)


def run_pass(client, spec, pass_no):
    rows = []
    for idx, case in enumerate(spec["evaluation"]):
        # Fresh identity per case keeps every item independent of dialogue/history state.
        context = client.create_client_context("bench_p%d_%04d" % (pass_no, idx + 1))
        t0 = time.perf_counter()
        response = client.process_question(context, case["text"])
        ms = (time.perf_counter() - t0) * 1000.0
        token = extract_token(response)
        predicted = token_to_intent(token, spec)
        rows.append({
            "id": case["id"],
            "track": case["track"],
            "text": case["text"],
            "expected": case["expected"],
            "predicted": predicted,
            "response_token": token,
            "raw_response": str(response),
            "ms": ms,
        })
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True, help="Harness root")
    args = ap.parse_args()
    root = Path(args.root).resolve()
    spec_path = root / "frozen" / "benchmark-spec.json"
    aiml_path = root / "frozen" / "bots" / "aiml2" / "benchmark.aiml"
    results = root / "results"
    results.mkdir(exist_ok=True)
    spec = load_spec(str(spec_path))

    version = package_version()
    if version != "3.6":
        raise RuntimeError("Expected Program-Y 3.6, got %s" % version)

    with tempfile.TemporaryDirectory(prefix="gvya-aiml-bench-") as td:
        category_dir = Path(td) / "categories"
        category_dir.mkdir(parents=True)
        # Copy only the frozen benchmark source. No packaged/default AIML categories are admitted.
        (category_dir / "benchmark.aiml").write_bytes(aiml_path.read_bytes())

        t0 = time.perf_counter()
        client = make_client(category_dir)
        load_ms = (time.perf_counter() - t0) * 1000.0
        first = run_pass(client, spec, 1)
        second = run_pass(client, spec, 2)

    p1 = [r["predicted"] for r in first]
    p2 = [r["predicted"] for r in second]
    if p1 != p2:
        for a, b in zip(first, second):
            if a["predicted"] != b["predicted"]:
                raise RuntimeError(
                    "Non-deterministic AIML result at %s: %r vs %r"
                    % (a["id"], a["predicted"], b["predicted"])
                )
        raise RuntimeError("Non-deterministic AIML predictions")

    summary = score_rows(
        "AIML 2.0",
        "Program-Y 3.6",
        spec,
        first,
        generated_source_bytes=aiml_path.stat().st_size,
        extra={
            "implementation": "Program-Y",
            "implementation_version": version,
            "load_ms": load_ms,
            "semantic_source": "frozen/bots/aiml2/benchmark.aiml",
            "semantic_isolation": "only frozen benchmark.aiml is configured as category knowledge; Program-Y parser node definitions are runtime grammar infrastructure",
            "session_isolation": "fresh Program-Y client context/user id per evaluation case",
            "execution_path": "ProgramYChatbot -> ConsoleBotClient -> Program-Y Brain/AIMLParser",
        },
    )
    write_jsonl(str(results / "aiml2.raw.jsonl"), first)
    write_json(str(results / "aiml2.summary.json"), summary)
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
