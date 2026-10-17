# Failing asserts: both builds must fail the same way, with the same messages. The actual value
# of the first assert is a secret, so it must be redacted in the messages too.

GET {{origin}}/auth/key
X-Api-Key: {{api_key}}
HTTP 200
[Asserts]
status == 201
header "X-Key-Echo" == "wrong-key"
header "X-Missing" exists
jsonpath "$.ok" == false
jsonpath "$.missing" exists
jsonpath "$.ok" isString
body contains "nope"
body matches /^\d+$/
bytes count == 1
xpath "string(//title)" == "x"
