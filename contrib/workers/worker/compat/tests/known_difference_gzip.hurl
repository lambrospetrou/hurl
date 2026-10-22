# compat: known-difference workerd's fetch decodes gzip and br bodies, so hurl-wasm removes their Content-Encoding header
# The suite expects the `known_difference_*.hurl` files to differ (one difference per file, since a
# run stops at the first failing entry). If one ever matches, remove its first line.

GET {{origin}}/text/gzip
[Options]
compressed: true
HTTP 200
[Asserts]
header "Content-Encoding" == "gzip"
body == "Hello compressed world!\n"
