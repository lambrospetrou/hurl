# Runtime errors: the run stops at the first one, like the CLI.

GET {{origin}}/text/plain
HTTP 200
[Captures]
not_json: jsonpath "$.a"

GET {{origin}}/text/plain
HTTP 200
[Asserts]
jsonpath "$.a" == 1

GET {{origin}}/redirect/5
[Options]
location: true
max-redirs: 2
HTTP 200
