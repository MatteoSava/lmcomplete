# Eval Dataset

Structured dataset for testing and iterating on lmc's system prompt and context assembly.

## Format

JSONL at `dataset.jsonl`. Each line:

```json
{
  "id": "git-001",
  "tier": "T1|T2|T3",
  "input": "natural language query",
  "expected": "expected shell command",
  "domain": "git|docker|curl|k8s|pkg|cloud|file|net|proc|text|misc",
  "context": {
    "cwd_type": "git|rust|node|go|docker|compose|k8s|make|python|terraform",
    "history": ["optional", "recent", "commands"],
    "git_branch": "optional-branch-name",
    "git_status": "optional short status"
  }
}
```

## Tiers

| Tier | Description | Source |
|------|-------------|--------|
| **T1** | Curated core — our target domains | Hand-written |
| **T2** | NL2Bash subset — standard Linux commands | Filtered from [NL2Bash](https://github.com/TellinaTool/nl2bash) (MIT) |
| **T3** | Context-aware — history/cwd dependent | Hand-written |

## Adding T2 (NL2Bash subset)

1. Clone [TellinaTool/nl2bash](https://github.com/TellinaTool/nl2bash)
2. Filter `data/bash/` for the top ~500 most common utility pairs
3. Convert to our JSONL format with `tier: "T2"` and appropriate `domain` tags
4. Append to `dataset.jsonl`

## Running evals

```sh
# Validate JSONL is parseable
python3 -c "import json; [json.loads(l) for l in open('eval/dataset.jsonl')]"

# Count by tier
jq -r '.tier' eval/dataset.jsonl | sort | uniq -c

# Count by domain
jq -r '.domain' eval/dataset.jsonl | sort | uniq -c
```
