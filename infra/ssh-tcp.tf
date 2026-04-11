# =============================================================================
# SSH TCP Passthrough via NGINX Ingress Controller
# =============================================================================
# Configures the RKE2 built-in NGINX ingress controller to listen on port 22
# and forward raw TCP traffic to the late-ssh pod on port 2222 with
# PROXY protocol metadata so the backend can see real client IPs.
# This enables: ssh late.sh
# =============================================================================

resource "kubernetes_manifest" "nginx_tcp_config" {
  manifest = {
    apiVersion = "helm.cattle.io/v1"
    kind       = "HelmChartConfig"
    metadata = {
      name      = "rke2-ingress-nginx"
      namespace = "kube-system"
    }
    spec = {
      valuesContent = yamlencode({
        tcp = {
          "22" = "default/service-ssh-sv:2222::PROXY"
        }
      })
    }
  }
}
