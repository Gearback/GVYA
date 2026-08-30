from __future__ import print_function
import json
import math
import os
import re
import time

TOKEN_RE = re.compile(r"BENCH:([A-Z0-9_]+)", re.I)


def load_spec(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def token_to_intent(token, spec):
    if token is None:
        return None
    token = token.upper()
    if token == "BENCH:FALLBACK":
        return None
    mapping = {row["answer_token"].upper(): row["id"] for row in spec["intents"]}
    return mapping.get(token)


def extract_token(text):
    if text is None:
        return None
    matches = TOKEN_RE.findall(str(text))
    if not matches:
        return None
    return "BENCH:" + matches[-1].upper()


def percentile(values, fraction):
    if not values:
        return None
    vals = sorted(values)
    idx = int(math.ceil(fraction * len(vals))) - 1
    idx = max(0, min(len(vals) - 1, idx))
    return vals[idx]


def score_rows(system, runtime, spec, rows, generated_source_bytes=None, extra=None):
    in_domain = [r for r in rows if r["expected"] is not None]
    ood = [r for r in rows if r["expected"] is None]
    correct = sum(1 for r in in_domain if r["predicted"] == r["expected"])
    wrong = sum(1 for r in in_domain if r["predicted"] is not None and r["predicted"] != r["expected"])
    ood_fp = sum(1 for r in ood if r["predicted"] is not None)

    tracks = {}
    for track in sorted(set(r["track"] for r in rows)):
        rs = [r for r in rows if r["track"] == track]
        c = sum(1 for r in rs if r["predicted"] == r["expected"])
        tracks[track] = {
            "n": len(rs),
            "correct": c,
            "accuracy": c / float(len(rs)),
            "non_null": sum(1 for r in rs if r["predicted"] is not None),
        }

    labels = [r["id"] for r in spec["intents"]] + [None]
    f1s = []
    for label in labels:
        tp = fp = fn = 0
        for row in rows:
            pred, expected = row["predicted"], row["expected"]
            if pred == label and expected == label:
                tp += 1
            elif pred == label and expected != label:
                fp += 1
            elif pred != label and expected == label:
                fn += 1
        precision = tp / float(tp + fp) if tp + fp else 0.0
        recall = tp / float(tp + fn) if tp + fn else 0.0
        f1s.append((2.0 * precision * recall / (precision + recall)) if precision + recall else 0.0)

    times = [float(r["ms"]) for r in rows if r.get("ms") is not None]
    out = {
        "system": system,
        "runtime": runtime,
        "generated_source_bytes": generated_source_bytes,
        "cases": len(rows),
        "in_domain_cases": len(in_domain),
        "ood_cases": len(ood),
        "strict_intent_accuracy_in_domain": correct / float(len(in_domain)),
        "correct_in_domain": correct,
        "wrong_intent_rate": wrong / float(len(in_domain)),
        "wrong_intent_count": wrong,
        "ood_false_positive_rate": ood_fp / float(len(ood)),
        "ood_false_positive_count": ood_fp,
        "macro_f1_including_fallback": sum(f1s) / float(len(f1s)),
        "deterministic_replay": True,
        "latency_ms": {
            "p50": percentile(times, 0.50),
            "p95": percentile(times, 0.95),
            "mean": sum(times) / float(len(times)) if times else None,
        },
        "tracks": tracks,
    }
    if extra:
        out.update(extra)
    return out


def write_jsonl(path, rows):
    with open(path, "w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")


def write_json(path, data):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2, sort_keys=True)
        f.write("\n")
