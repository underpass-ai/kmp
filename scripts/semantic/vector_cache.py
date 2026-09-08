"""Disposable exact-content vector cache partitioned by encoder revision."""
import sqlite3
import numpy as np


class VectorCache:
    def __init__(self, path, revision):
        self.revision = revision
        self.db = sqlite3.connect(path)
        self.db.execute('CREATE TABLE IF NOT EXISTS vectors (revision TEXT, hash TEXT, dimensions INTEGER, vector BLOB, PRIMARY KEY(revision, hash))')

    def get(self, fingerprint):
        row = self.db.execute('SELECT dimensions, vector FROM vectors WHERE revision=? AND hash=?',
                              (self.revision, fingerprint)).fetchone()
        if row is None:
            return None
        vector = np.frombuffer(row[1], dtype='<f4')
        if len(vector) != row[0] or not np.isfinite(vector).all() or np.linalg.norm(vector) == 0:
            raise ValueError('invalid cached vector')
        return vector

    def put_many(self, pairs):
        with self.db:
            for fingerprint, vector in pairs:
                self.db.execute('INSERT OR REPLACE INTO vectors VALUES (?, ?, ?, ?)',
                    (self.revision, fingerprint, len(vector), np.asarray(vector, dtype='<f4').tobytes()))
