#!/usr/bin/env bash
# Regenerate and verify local KMP/MCP E2E tooling before live replay runs.
# Reference: docs/enterprise/operations.md

set -uo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/../.." && pwd)
# shellcheck source=scripts/e2e/lib.sh
source "$SCRIPT_DIR/lib.sh"

parse_common_flags "$@"

NAMESPACE=${NAMESPACE:-underpass-runtime}
KERNEL_RELEASE=${KERNEL_RELEASE:-kmp}
KERNEL_POD_SELECTOR=${KERNEL_POD_SELECTOR:-app.kubernetes.io/name=kmp,app.kubernetes.io/component=server}
NEO4J_POD_SELECTOR=${NEO4J_POD_SELECTOR:-app.kubernetes.io/name=kmp,app.kubernetes.io/component=neo4j}
KMP_PUBLIC_ENDPOINT=${KMP_PUBLIC_ENDPOINT:-https://kmp.underpassai.com}
KMP_GRPCURL_TARGET=${KMP_GRPCURL_TARGET:-kmp.underpassai.com:443}
CLIENT_CERT=${CLIENT_CERT:-/tmp/client.crt}
CLIENT_KEY=${CLIENT_KEY:-/tmp/client.key}
REAL_ANCHOR=${REAL_ANCHOR:-article:incident:checkout-latency:20260504T233722Z:frontend}

if [[ "$ALLOW_REDEPLOY" == "1" ]]; then
  warn "redeploy flag" "--redeploy accepted but this script is preflight-only; deploy via existing runbooks"
fi
if [[ "$ALLOW_REINSTALL_ADAPTERS" == "1" ]]; then
  warn "adapter reinstall flag" "not applicable in kmp"
fi

install_rehydration_mcp() {
  if [[ -d crates/kmp-mcp-bin ]]; then
    cargo install --path crates/kmp-mcp-bin --force
  elif [[ -d crates/kmp-mcp ]]; then
    cargo install --path crates/kmp-mcp --force
  else
    printf 'No kmp-mcp crate path found\n' >&2
    return 1
  fi
}

check_branch_state "$REPO_ROOT" "git state"
run_checked "cargo release build" "workspace release build completed" cargo build --workspace --release
run_checked "cargo install mcp" "kmp-mcp installed to user cargo bin" install_rehydration_mcp
check_binary_freshness "kmp-mcp" "$REPO_ROOT" "kmp-mcp freshness"

live_image=$(kubectl -n "$NAMESPACE" get deploy "$KERNEL_RELEASE" -o jsonpath='{.spec.template.spec.containers[*].image}' 2>/tmp/kernel-live-image.err || true)
manifest_image=$(helm get manifest "$KERNEL_RELEASE" -n "$NAMESPACE" 2>/tmp/kernel-helm-manifest.err | awk '/image:/ {gsub(/"/, "", $2); print $2; exit}' || true)
if [[ -z "$live_image" ]]; then
  fail "kernel live image" "could not read deployment image: $(cat /tmp/kernel-live-image.err)"
elif [[ -z "$manifest_image" ]]; then
  warn "kernel helm image" "could not infer image from helm manifest; live image is $live_image"
elif [[ "$live_image" == *"$manifest_image"* || "$manifest_image" == *"$live_image"* ]]; then
  ok "kernel image drift" "deployment image matches Helm manifest: $live_image"
else
  fail "kernel image drift" "deployment image '$live_image' differs from Helm manifest '$manifest_image'"
fi

if kubectl -n "$NAMESPACE" get pod -l "$KERNEL_POD_SELECTOR" -o wide >/tmp/kernel-pods.txt 2>&1; then
  ok "kernel pod describe" "kernel pods query succeeded"
  if [[ "$VERBOSE" == "1" ]]; then
    kubectl -n "$NAMESPACE" describe pod -l "$KERNEL_POD_SELECTOR"
  fi
else
  fail "kernel pod describe" "kubectl query failed: $(cat /tmp/kernel-pods.txt)"
fi

if kubectl -n "$NAMESPACE" wait --for=condition=Ready pod -l "$NEO4J_POD_SELECTOR" --timeout=5s >/tmp/neo4j-ready.txt 2>&1; then
  ok "neo4j ready" "Neo4j pod is Running and Ready"
else
  fail "neo4j ready" "Neo4j pod not ready: $(cat /tmp/neo4j-ready.txt)"
fi

if command -v grpcurl >/dev/null 2>&1; then
  if grpcurl -cert "$CLIENT_CERT" -key "$CLIENT_KEY" -insecure "$KMP_GRPCURL_TARGET" list >/tmp/kernel-grpcurl.txt 2>&1; then
    ok "kernel grpc reachability" "grpcurl reached $KMP_GRPCURL_TARGET"
  else
    fail "kernel grpc reachability" "grpcurl failed: $(cat /tmp/kernel-grpcurl.txt)"
  fi
else
  if curl -sSkI "$KMP_PUBLIC_ENDPOINT" >/tmp/kernel-curl.txt 2>&1; then
    warn "kernel grpc reachability" "grpcurl missing; curl reached $KMP_PUBLIC_ENDPOINT but did not validate gRPC"
  else
    fail "kernel grpc reachability" "grpcurl missing and curl failed: $(cat /tmp/kernel-curl.txt)"
  fi
fi

if command -v kmp-mcp >/dev/null 2>&1; then
  request=$(printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kmp_wake","arguments":{"about":"%s"}}}' "$REAL_ANCHOR")
  if response=$(printf '%s\n' "$request" | KMP_MCP_BACKEND=grpc KMP_KERNEL_GRPC_ENDPOINT="$KMP_PUBLIC_ENDPOINT" kmp-mcp 2>&1); then
    if printf '%s' "$response" | grep -Fq '"isError":false'; then
      ok "kmp-mcp smoke" "kmp_wake succeeded for $REAL_ANCHOR"
    else
      fail "kmp-mcp smoke" "kmp_wake did not return isError=false: $response"
    fi
  else
    fail "kmp-mcp smoke" "kmp-mcp command failed: $response"
  fi
else
  fail "kmp-mcp smoke" "kmp-mcp not on PATH after install"
fi

finish_summary
