import json
from scripts.performance.memory_bench.domain.metrics import RetrievalOutcome, RetrievalScorecard
cases = [
 ("first_rank_hit", ["a"], ["a","x"], ["a"], False, 4096, 12),
 ("third_rank_hit", ["a"], ["x","y","a"], [], False, 2048, 30),
 ("absent_from_results", ["a"], ["x","y"], [], False, 1000, 5),
 ("half_of_two_judged", ["a","b"], ["a","x"], ["b"], False, 300, 7),
 ("duplicate_hit_earns_twice_in_ndcg", ["a"], ["a","a","x"], ["a"], False, 0, 0),
 ("duplicate_counts_once_in_recall", ["a","b"], ["a","a"], [], False, 0, 0),
 ("honest_unknown_without_judged", [], [], [], True, 120, 3),
 ("false_unknown_with_judged", ["a"], [], [], True, 150, 4),
 ("cited_but_not_judged", ["a"], ["a"], ["z"], False, 10, 1),
 ("hits_beyond_cutoffs", ["k","l","m"], [f"x{i}" for i in range(9)]+["k","l","m"], ["m"], False, 9999, 250),
 ("three_judged_all_in_top_five", ["c","a","b"], ["a","q","b","c","r"], ["a","c"], False, 512, 2),
 ("colon_refs_as_the_scorecard_sees_them", ["project:x:e2","project:x:e1"], ["project:x:e2","project:x:e9","project:x:e1"], ["project:x:e1"], False, 4011, 46),
]
out = {"schema":"kmp.bench.metric_parity.v1",
 "about":"Shared fixture: the same ranking outcomes scored by crates/kmp-testkit/src/retrieval_scorecard.rs and scripts/performance/memory_bench/domain/metrics.py must give these numbers. Floats compare within `tolerance`.",
 "tolerance":1e-12, "cases":[], "scorecard":None}
outs=[]
for name,j,r,c,u,b,e in cases:
    o=RetrievalOutcome.of(j,r,c,u,b,e); outs.append(o)
    out["cases"].append({"name":name,"judged":j,"retrieved":r,"cited":c,"unknown":u,"used_bytes":b,"elapsed_millis":e,
      "expected":{"recall_at_1":o.recall_at(1),"recall_at_5":o.recall_at(5),"recall_at_10":o.recall_at(10),
       "reciprocal_rank":o.reciprocal_rank(),"ndcg_at_10":o.ndcg_at(10),"answer_cites_judged":o.answer_cites_judged(),
       "is_false_unknown":o.is_false_unknown(),"has_complete_support_at_5":o.has_complete_support_at(5)}})
s=RetrievalScorecard.score(outs)
out["scorecard"]={k:getattr(s,k) for k in ("cases","recall_at_1","recall_at_5","recall_at_10","mean_reciprocal_rank","ndcg_at_10","answer_core_precision","false_unknown_rate","mean_used_bytes","mean_elapsed_millis")}
with open("crates/kmp-testkit/judged/metric_parity.json","w") as f:
    json.dump(out,f,indent=2); f.write("\n")
