#!/usr/bin/env python3
"""Independent nontrivial runtime-loadable GVYA v5 fixture.

Exercises the executable IR shape for semantic resolution -> response -> capability proposal and
asset lookup. It is a validation fixture, never a production compiler.
"""
from __future__ import annotations
import hashlib, json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from gvya_container_reference import Entry, build, canonical

profile_set_keys=["canonical_suffix_exceptions","detached_suffixes","normalization_remove_chars","pure_glue","very_low_weight","low_weight","context_low_weight","generic_singletons","reporting_verbs","reporting_nouns","pronouns","negations","social_vocabulary","task_cues","weak_numeric_ignore","continuation_exact_phrases","continuation_question_starters","continuation_references","generic_followup_phrases","time_markers"]
profile_map_keys=["canonical_tokens","canonical_suffixes","normalization_rewrites","colloquial","number_words","relative_dates","colors","units","origin_prefixes"]
profile={key:[] for key in profile_set_keys}
profile.update({key:{} for key in profile_map_keys})
profile["pattern_sets"]={}
profile["boolean_values"]={}
profile["custom_entities"]={}
meaning="hello"
response={
 "id":"hello.response","kind":"normal","texts":[{"language":"en","variants":["Hello from GVYA."]}],
 "conditions":[],"hint_level":None,"repeat_stage":None,"effects":[],"opens_followup":None,
 "extra_messages":[],"assets":[{"asset_id":"tone","alt_text":"GVYA fixture tone"}],"links":[],
}
empty_object_schema={"type":"object","properties":{},"required":[],"additional_properties":False,"min_properties":None,"max_properties":None}
program={
 "format":"gvya.program","version":1,"project_id":"runtime-action","brain_id":"runtime-action","enabled_languages":["en"],"default_language":"en",
 "source_packages":{},"packages":[],
 "semantic":{
   "config":{"candidate_limit":120,"resolution_threshold":0.45,"ambiguity_margin":0.04,"resolver_min_confidence":0.55,"resolver_candidate_limit":8},
   "profiles":{"en":profile},
   "patterns":[{"id":meaning,"class":"general","patterns":[],"samples":[{"language":"en","text":"hello"}],"negative_samples":[],"retrieval_terms":[],"priority":0,"positive_assumption":False,"slots":[],"references":[]}],
 },
 "conversation":{
   "config":{"default_topic_ttl":3,"default_followup_ttl":2,"recent_response_limit":8,"recent_variant_limit":8,"recent_user_window":8,"repeat_detection_window":8,"repeat_detection_threshold":4,"max_messages_per_turn":6,"repair_candidate_min_score":0.4,"author_numbers":[],"topic_preference_margin":0.08},
   "behaviors":[{"id":"hello.behavior","meaning":meaning,"topic":None,"topic_scoped":False,"activates_topic":False,"topic_ttl":None,"followup_scope":None,"repair_continuation_candidate":False,"repeat_same_input_after":None,"repeat_same_meaning_after":None,"requires_values":[],"forbidden_values":[],"responses":[response]}],
   "capability_result_behaviors":[{
     "id":"wave.result","capability":"demo.wave","capability_version":"1","succeeded":True,"error_code":None,
     "responses":[{
       "id":"wave.result.response","kind":"normal","texts":[{"language":"en","variants":["Wave completed."]}],
       "conditions":[],"hint_level":None,"repeat_stage":None,"effects":[],"opens_followup":None,
       "extra_messages":[],"assets":[],"links":[],
     }],
   }],
   "openings":[],"fallback_behaviors":[],
   "style_lexicon":{"formal_terms":[],"informal_terms":[]},
 },
 "capabilities":{
   "definitions":[{
     "contract":{"id":"demo.wave","version":"1","title":"Wave","description":"Fixture capability","input_schema":{"type":"object","properties":{},"additionalProperties":False},"output_schema":None,"reference_kinds":[],"effect_class":"pure","confirmation_hint":"never"},
     "input_shape":empty_object_schema,"output_shape":None,"host_effects":[],
   }],
   "bindings":[{"id":"hello.wave","trigger":{"meaning":meaning,"behavior":None,"response":None},"capability":"demo.wave","arguments":[]}],
   "policies":[],
   "config":{"schema_limits":{"max_depth":16,"max_array_items":256,"max_object_properties":128,"max_string_bytes":8192,"max_errors":16},"max_proposals_per_turn":8,"max_bindings":2048,"max_policy_rules":4096},
 },
 "assets":[],
}
asset=b"GVYA runtime fixture asset\n"
asset_sha=hashlib.sha256(asset).hexdigest()
program["assets"]=[{"id":"tone","media_type":"application/octet-stream","logical_path":"assets/tone.bin","digest":asset_sha}]
program_bytes=canonical(program); program_sha=hashlib.sha256(program_bytes).hexdigest()
integrity={"format":"gvya.integrity","version":1,"program":{"path":"program.json","sha256":program_sha,"size":len(program_bytes)},"assets":[{"path":"assets/tone.bin","sha256":asset_sha,"size":len(asset)}],"source_packages":[]}
integrity_bytes=canonical(integrity)
manifest={
 "format":"gvya.artifact","version":1,"container_version":1,"project_id":"runtime-action","brain_id":"runtime-action",
 "program":{"path":"program.json","format":"gvya.program","version":1,"sha256":program_sha,"size":len(program_bytes)},
 "integrity":{"path":"integrity.json","sha256":hashlib.sha256(integrity_bytes).hexdigest()},
 "packages":[],"assets":[{"id":"tone","path":"assets/tone.bin","media_type":"application/octet-stream","sha256":asset_sha,"size":len(asset)}],"debug_map":None,
 "signing":{"content_root_algorithm":"sha256-essential-entry-set-v1","envelope_path":"signature.json"},
}
artifact=build([
 Entry(1,"manifest.json",True,canonical(manifest)),Entry(2,"program.json",True,program_bytes),
 Entry(3,"assets/tone.bin",True,asset),Entry(6,"integrity.json",True,integrity_bytes),
])
out=pathlib.Path(sys.argv[1] if len(sys.argv)>1 else "validation/fixtures/runtime-action.gvya")
out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(artifact)
print(f"{out} sha256={hashlib.sha256(artifact).hexdigest()} bytes={len(artifact)}")
