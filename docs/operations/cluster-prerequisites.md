# Cluster Prerequisites

Infrastructure required before deploying the kernel with mTLS and external access.

## MetalLB (bare-metal load balancer)

The cluster needs a load balancer to assign external IPs to `LoadBalancer` services.
MetalLB provides this for bare-metal clusters.

### Install

```bash
kubectl apply -f https://raw.githubusercontent.com/metallb/metallb/v0.14.9/config/manifests/metallb-native.yaml
```

### Configure IP pool

```bash
kubectl apply -f - <<EOF
apiVersion: metallb.io/v1beta1
kind: IPAddressPool
metadata:
  name: default-pool
  namespace: metallb-system
spec:
  addresses:
    - 192.168.1.240-192.168.1.250
---
apiVersion: metallb.io/v1beta1
kind: L2Advertisement
metadata:
  name: default
  namespace: metallb-system
spec:
  ipAddressPools:
    - default-pool
EOF
```

Adjust the IP range to your network. The NGINX ingress controller will get
an IP from this pool.

### Verify

```bash
kubectl get svc -n ingress-nginx ingress-nginx-controller
# Should show EXTERNAL-IP from the pool (e.g. 192.168.1.241)
```

## NGINX Ingress Controller

Handles external gRPC traffic with TLS termination.

### Install

```bash
helm repo add ingress-nginx https://kubernetes.github.io/ingress-nginx
helm install ingress-nginx ingress-nginx/ingress-nginx \
  --namespace ingress-nginx --create-namespace
```

### Verify

```bash
kubectl get svc -n ingress-nginx
# ingress-nginx-controller should have a LoadBalancer EXTERNAL-IP
```

## cert-manager

Issues and auto-rotates TLS certificates.

### Install

```bash
helm repo add jetstack https://charts.jetstack.io
helm install cert-manager jetstack/cert-manager \
  --namespace cert-manager --create-namespace \
  --set crds.enabled=true
```

### Create Let's Encrypt issuer with Route 53

Requires AWS credentials with Route 53 permissions for DNS-01 challenge.

```bash
# Create AWS credentials secret
kubectl create secret generic prod-route53-credentials-secret \
  -n cert-manager \
  --from-literal=access-key-id=<AWS_ACCESS_KEY_ID> \
  --from-literal=secret-access-key=<AWS_SECRET_ACCESS_KEY>

# Create ClusterIssuer
kubectl apply -f - <<EOF
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt-prod-r53
spec:
  acme:
    email: your-email@example.com
    server: https://acme-v02.api.letsencrypt.org/directory
    privateKeySecretRef:
      name: letsencrypt-prod-account-key
    solvers:
      - dns01:
          route53:
            region: eu-south-2
            hostedZoneID: <YOUR_HOSTED_ZONE_ID>
            accessKeyIDSecretRef:
              name: prod-route53-credentials-secret
              key: access-key-id
            secretAccessKeySecretRef:
              name: prod-route53-credentials-secret
              key: secret-access-key
EOF
```

### Verify

```bash
kubectl get clusterissuer letsencrypt-prod-r53
# STATUS should be True
```

## Route 53 DNS

The kernel ingress needs a DNS record pointing to the MetalLB IP.

### Create A record

```bash
ZONE_ID=<YOUR_HOSTED_ZONE_ID>
METALLB_IP=<EXTERNAL_IP_FROM_NGINX>

aws route53 change-resource-record-sets --hosted-zone-id $ZONE_ID --change-batch "{
  \"Changes\": [{
    \"Action\": \"UPSERT\",
    \"ResourceRecordSet\": {
      \"Name\": \"kmp.underpassai.com\",
      \"Type\": \"A\",
      \"TTL\": 300,
      \"ResourceRecords\": [{\"Value\": \"$METALLB_IP\"}]
    }
  }]
}"
```

### Verify

```bash
aws route53 list-resource-record-sets --hosted-zone-id $ZONE_ID \
  --query "ResourceRecordSets[?Name=='kmp.underpassai.com.']"
```

## AWS CLI

Required for Route 53 DNS management.

### Install

```bash
curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o awscliv2.zip
unzip awscliv2.zip && sudo ./aws/install
```

### Configure

```bash
aws configure
# AWS Access Key ID: <key>
# AWS Secret Access Key: <secret>
# Default region: eu-south-2
# Default output: json
```

### Verify

```bash
aws route53 list-hosted-zones --query 'HostedZones[*].Name' --output text
```

## Summary

After these prerequisites, you should have:

| Component | What it provides | Verify |
|:----------|:-----------------|:-------|
| MetalLB | External IPs for LoadBalancer services | `kubectl get svc -n ingress-nginx` shows EXTERNAL-IP |
| NGINX Ingress | gRPC TLS termination + routing | Ingress controller pod running |
| cert-manager | Certificate issuance + auto-rotation | `kubectl get clusterissuer` shows Ready=True |
| Route 53 | DNS resolution for ingress hostname | `dig kmp.underpassai.com` resolves |
| AWS CLI | Route 53 management | `aws route53 list-hosted-zones` works |

Next: [mTLS Deployment Guide](mtls-deployment.md)
