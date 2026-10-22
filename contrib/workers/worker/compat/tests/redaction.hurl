# Secrets (`--secret api_key=...`) and `redact` captures must be replaced by `***` everywhere in
# the JSON report: URLs, query strings, request and response headers, captures and curl commands.

POST {{origin}}/auth/login
{"user": "alice", "password": "s3cret"}
HTTP 200
[Captures]
token: jsonpath "$.token" redact
[Asserts]
jsonpath "$.token" startsWith "tok-"
cookie "session" == "sess-77aa"

GET {{origin}}/auth/me
Authorization: Bearer {{token}}
HTTP 200
[Asserts]
jsonpath "$.user" == "alice"
jsonpath "$.session" == "sess-77aa"

GET {{origin}}/auth/key
X-Api-Key: {{api_key}}
HTTP 200
[Captures]
echoed_key: header "X-Key-Echo"
[Asserts]
header "X-Key-Echo" == "{{api_key}}"
jsonpath "$.ok" == true

GET {{origin}}/auth/key?key={{api_key}}
HTTP 200

GET {{origin}}/auth/key
[Query]
key: {{api_key}}
HTTP 200

GET {{origin}}/html/page
HTTP 200
[Captures]
csrf: xpath "string(//input[@name='csrf']/@value)" redact

POST {{origin}}/echo
[Form]
csrf: {{csrf}}
token: {{token}}
HTTP 200
[Asserts]
jsonpath "$.body" contains "csrf-"
