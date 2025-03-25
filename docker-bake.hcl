variable "GITHUB_REPO" {
  default = "https://github.com/sigp/eleel"
}

group "default" {
  targets = ["binary-amd64", "binary-arm64"]
  labels = {
    "org.opencontainers.image.source" = "{GITHUB_REPO}"
  }
  context = "."
  attest = [
    "type=provenance,mode=max",
    "type=sbom",
  ]
}

target "binary-amd64" {
  dockerfile = "Dockerfile.cross"
  context = "."
  platforms = ["linux/amd64"]
  args = {
    TARGET_ARCH = "x86_64-unknown-linux-gnu"
  }
}

target "binary-arm64" {
  dockerfile = "Dockerfile.cross"
  context = "."
  platforms = ["linux/arm64"]
  args = {
    TARGET_ARCH = "aarch64-unknown-linux-gnu"
  }
}

target "manifest" {
  inherits = ["binary"]
  output = ["type=registry"]
  description = "Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus clients to connect to a single execution client."
}
