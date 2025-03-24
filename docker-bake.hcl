variable "ARCH" {
  default = "amd64"
}

group "default" {
  targets = ["binary"]
  context = "."
  attest = [
    "type=provenance,mode=max",
    "type=sbom",
  ]
}

target "binary" {
  dockerfile = "Dockerfile.cross"
  context = "."
  
}

target "manifest" {
  inherits = ["binary"]
  platforms = ["linux/${ARCH}"]
  output = ["type=registry"]
  description = "Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus clients to connect to a single execution client."
}
