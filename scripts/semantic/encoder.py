"""Local Qwen3 embedding encoder; no remote code or network model loading."""
import numpy as np
import torch
from transformers import AutoModel, AutoTokenizer


class Encoder:
    instruction = 'Given a question about past conversations, retrieve passages that provide evidence to answer it.'

    def __init__(self, model_path):
        torch.manual_seed(0)
        self.tokenizer = AutoTokenizer.from_pretrained(model_path, padding_side='left', local_files_only=True)
        self.model = AutoModel.from_pretrained(model_path, dtype=torch.bfloat16,
            attn_implementation='sdpa', local_files_only=True).to('cuda').eval()

    def encode(self, texts):
        vectors = []
        for start in range(0, len(texts), 16):
            batch = self.tokenizer(texts[start:start+16], padding=True, truncation=False, return_tensors='pt')
            if batch['input_ids'].shape[1] > 32768:
                raise ValueError('source exceeds encoder context; truncation forbidden')
            with torch.inference_mode():
                states = self.model(**{k: v.to('cuda') for k, v in batch.items()}).last_hidden_state[:, -1]
                vectors.append(torch.nn.functional.normalize(states.float(), p=2, dim=1).cpu().numpy())
        return np.concatenate(vectors)

    def query(self, question):
        return self.encode([f'Instruct: {self.instruction}\nQuery: {question}'])[0]
