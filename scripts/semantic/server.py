"""Optional local retrieval sidecar. Model weights must already be on disk."""
import argparse
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from pathlib import Path
import time

from encoder import Encoder
from retriever import Retriever
from vector_cache import VectorCache


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send_json(self, status, body):
        encoded = json.dumps(body).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def do_GET(self):
        self.send_json(200 if self.path == '/health' else 404,
                       {'model_revision': self.server.retriever.revision})

    def do_POST(self):
        start = time.monotonic()
        if self.path != '/rank':
            self.send_json(404, {'error': 'unknown endpoint'})
            return
        try:
            size = int(self.headers.get('Content-Length', '0'))
            if not 0 < size <= 16 * 1024 * 1024:
                raise ValueError('invalid request size')
            request = json.loads(self.rfile.read(size))
            result, metrics = self.server.retriever.rank(request)
            self.send_json(200, result)
            print(json.dumps({**metrics, 'elapsed_seconds': time.monotonic()-start}), flush=True)
        except (ValueError, KeyError, TypeError):
            self.send_json(400, {'error': 'invalid retrieval request'})
        except Exception:
            self.send_json(503, {'error': 'local encoder unavailable'})


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--model', required=True)
    parser.add_argument('--model-revision', required=True)
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--host', default='127.0.0.1')
    parser.add_argument('--port', type=int, default=8001)
    args = parser.parse_args()
    args.cache.parent.mkdir(parents=True, exist_ok=True)
    server = HTTPServer((args.host, args.port), Handler)
    server.retriever = Retriever(Encoder(args.model), VectorCache(args.cache, args.model_revision), args.model_revision)
    print(json.dumps({'status': 'ready', 'model_revision': args.model_revision}), flush=True)
    server.serve_forever()
