# Request bodies and request sections: JSON, form, multipart, files, raw, base64, hex, XML,
# GraphQL, [BasicAuth], [Cookies], [Query] encoding and HTTP methods.

POST {{origin}}/echo
Content-Type: application/json
X-Custom: custom value
{"a": 1, "b": [true, null], "c": {"d": "é"}}
HTTP 200
[Asserts]
jsonpath "$.method" == "POST"
jsonpath "$.headers['content-type']" == "application/json"
jsonpath "$.headers['x-custom']" == "custom value"
jsonpath "$.json.c.d" == "é"
jsonpath "$.json.b[1]" == null

POST {{origin}}/echo
[Form]
a: 1
text: hello world & more
unicode: café
HTTP 200
[Asserts]
jsonpath "$.headers['content-type']" == "application/x-www-form-urlencoded"
jsonpath "$.body" == "a=1&text=hello%20world%20%26%20more&unicode=caf%C3%A9"

POST {{origin}}/echo
[Multipart]
field: value
file: file,data/hello.txt;
json: file,data/user.json; application/json
HTTP 200
[Asserts]
jsonpath "$.headers['content-type']" startsWith "multipart/form-data; boundary="
jsonpath "$.body" contains "name=\"field\"\r\n\r\nvalue\r\n"
jsonpath "$.body" contains "filename=\"data/hello.txt\"\r\nContent-Type: text/plain\r\n\r\nHello from a file!\n"
jsonpath "$.body" contains "Content-Type: application/json\r\n\r\n{\"name\":\"from file\""

POST {{origin}}/echo
Content-Type: application/json
file,data/user.json;
HTTP 200
[Asserts]
jsonpath "$.json.name" == "from file"
jsonpath "$.json.tags" count == 2

PUT {{origin}}/echo
Content-Type: text/plain
```
line one
line two {{origin}}
```
HTTP 200
[Asserts]
jsonpath "$.method" == "PUT"
jsonpath "$.body" startsWith "line one\nline two http://"

POST {{origin}}/echo
base64,SGVsbG8gYmFzZTY0;
HTTP 200
[Asserts]
jsonpath "$.body" == "Hello base64"

POST {{origin}}/echo
hex,48656c6c6f20686578;
HTTP 200
[Asserts]
jsonpath "$.body" == "Hello hex"

POST {{origin}}/echo
`single line`
HTTP 200
[Asserts]
jsonpath "$.body" == "single line"

POST {{origin}}/echo
Content-Type: application/xml
<?xml version="1.0"?>
<user id="1"><name>Alice</name></user>
HTTP 200
[Asserts]
jsonpath "$.body" contains "<name>Alice</name>"

POST {{origin}}/echo
```graphql
query User($id: ID!) {
  user(id: $id) { name }
}

variables {
  "id": "1"
}
```
HTTP 200
[Asserts]
jsonpath "$.json.query" contains "user(id: $id)"
jsonpath "$.json.variables.id" == "1"

GET {{origin}}/echo
[BasicAuth]
bob: secret
HTTP 200
[Asserts]
jsonpath "$.headers['authorization']" == "Basic Ym9iOnNlY3JldA=="

GET {{origin}}/echo
[Cookies]
a: 1
b: two
HTTP 200
[Asserts]
jsonpath "$.cookies.a" == "1"
jsonpath "$.cookies.b" == "two"

GET {{origin}}/echo?x=1&x=2
[Query]
q: a b&c=d
empty:
unicode: Ελλάδα
HTTP 200
[Asserts]
jsonpath "$.query.q" == "a b&c=d"
jsonpath "$.query.empty" == ""
jsonpath "$.query.unicode" == "Ελλάδα"

DELETE {{origin}}/echo
HTTP 200
[Asserts]
jsonpath "$.method" == "DELETE"

OPTIONS {{origin}}/echo
HTTP 200
[Asserts]
jsonpath "$.method" == "OPTIONS"

PATCH {{origin}}/echo
{"patch": true}
HTTP 200
[Asserts]
jsonpath "$.method" == "PATCH"
jsonpath "$.json.patch" == true
