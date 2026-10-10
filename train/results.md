# Kai training results

One row per `python train/eval.py` run: a model or adapter on the held-out
bench tasks (train/heldout.txt), measured with `kai bench`. Every number comes
from the `kai bench` results JSON named in the last column. Nothing has been
trained yet, so there are no rows.

For the base model on the same held-out subset, `python train/eval.py
--from-json bench/results/<file>.json --dry-run` prints the row it would add.

| Date | Label | Backend | Model | Held-out tasks run | Passed | Success | Steps/task | s/step | Results file |
|---|---|---|---|---|---|---|---|---|---|
