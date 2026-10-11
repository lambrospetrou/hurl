# compat: known-difference without a Path attribute, libcurl reports the cookie path with a trailing slash ("/known/cookie/"), hurl-wasm without it like RFC 6265 ("/known/cookie")
# Only the reported path differs: both send the cookie to the same URLs.
GET {{origin}}/known/cookie/default-path
HTTP 200
[Asserts]
cookie "nopath" == "1"

GET {{origin}}/known/cookie/other
HTTP 404
[Asserts]
cookie "nopath" not exists
