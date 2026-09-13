"""Run after compiling and validating; never mix compilation with timing."""
import hashlib, json, os, subprocess
from pathlib import Path
root = Path(__file__).resolve().parents[2]
out = Path(__file__).resolve().parent
binary = root / 'target/debug/examples/node_batch_benchmark'
env = dict(os.environ, TMPDIR=str(root/'tmp'))
(out/'binary.json').write_text(json.dumps({'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'profile':'dev, unoptimized; shared workspace Cargo config in environment.json'},indent=2)+'\n')
with (out/'equivalence.json').open('w') as f:
    subprocess.run([str(binary),'verify'],cwd=root,env=env,stdout=f,check=True)
# Reverse the order in the second round; retain every sample and first read.
order = ['baseline','batch','baseline-http','batch-http', 'batch-http','baseline-http','batch','baseline']
for index, mode in enumerate(order):
    name = f'{index+1:02}-{mode}'
    with (out/f'{name}.json').open('w') as f:
        subprocess.run(['/usr/bin/time','-v','-o',str(out/f'{name}.time'),str(binary),mode,'20'],cwd=root,env=env,stdout=f,check=True)
    print(name+' complete', flush=True)
