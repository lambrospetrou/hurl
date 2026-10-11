# The [Options] section: redirects, retries, delays, timeouts, auth, headers, repeat, skip,
# variables and compression.

GET {{origin}}/redirect/3
HTTP 302
[Asserts]
header "Location" == "/redirect/2"

GET {{origin}}/redirect/3
[Options]
location: true
max-redirs: 3
HTTP 200
[Asserts]
redirects count == 3
url endsWith "/redirect/0"
body == "redirect done"

GET {{origin}}/redirect/2?status=301
[Options]
location: true
HTTP 200

POST {{origin}}/redirect-post
[Options]
location: true
```
posted body
```
HTTP 200
[Asserts]
jsonpath "$.method" == "POST"
jsonpath "$.body" == "posted body\n"

GET {{origin}}/flaky?id=options
[Options]
retry: 5
retry-interval: 10
HTTP 200
[Asserts]
jsonpath "$.attempts" == 3

GET {{origin}}/flaky?id=options-retry-assert&failures=1
[Options]
retry: 2
retry-interval: 1ms
HTTP *
[Asserts]
status == 200

GET {{origin}}/text/plain
[Options]
delay: 50ms
HTTP 200

GET {{origin}}/slow?ms=50
[Options]
max-time: 5s
HTTP 200
[Asserts]
jsonpath "$.slow" == true

GET {{origin}}/basic-auth
[Options]
user: bob:secret
HTTP 200
[Asserts]
body == "Authenticated as bob"

GET {{origin}}/basic-auth
HTTP 401
[Asserts]
header "WWW-Authenticate" == "Basic realm=\"compat\""

GET {{origin}}/echo
[Options]
header: X-From-Options: yes
header: X-Second: 2
HTTP 200
[Asserts]
jsonpath "$.headers['x-from-options']" == "yes"
jsonpath "$.headers['x-second']" == "2"

GET {{origin}}/counter
[Options]
repeat: 3
HTTP 200

GET {{origin}}/counter
HTTP 200
[Asserts]
jsonpath "$.count" == 4

GET {{origin}}/api/error?status=500
[Options]
skip: true
HTTP 200

GET {{origin}}/echo?greeting={{greeting}}&n={{n}}
[Options]
variable: greeting=hello
variable: n=7
HTTP 200
[Asserts]
jsonpath "$.query.greeting" == "hello"
variable "n" == 7

GET {{origin}}/echo
[Options]
verbose: true
very-verbose: false
HTTP 200
