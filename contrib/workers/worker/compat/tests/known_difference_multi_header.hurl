# compat: known-difference fetch's Headers merges the values of a repeated response header into one "a, b" value
GET {{origin}}/known/multi-header
HTTP 200
[Asserts]
header "X-Multi" count == 2
header "X-Multi" includes "b"
