group "default" {
  targets = ["binary"]
}

target "binary" {
  dockerfile = "Dockerfile.cross"
  context = "."
}

target "manifest" {
  inherits = ["binary"]
  platforms = ["linux/arm64", "linux/amd64"]
  output = ["type=registry"]
}
