group "default" {
  targets = ["binary"]
  context = "."
}

target "binary" {
  dockerfile = "Dockerfile.cross"
  context = "."
  name = "Eleel"
  description = "Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus clients to connect to a single execution client."
}

target "manifest" {
  inherits = ["binary"]
  platforms = ["linux/arm64", "linux/amd64"]
  output = ["type=registry"]
}
