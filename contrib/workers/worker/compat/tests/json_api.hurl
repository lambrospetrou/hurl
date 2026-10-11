# JSON REST API: CRUD, query parameters, pagination, JSON types and JSONPath.

GET {{origin}}/api/users
HTTP 200
Content-Type: application/json
X-Total-Count: 3
[Asserts]
jsonpath "$.total" == 3
jsonpath "$.users" count == 3
jsonpath "$.users[0].name" == "Alice"
jsonpath "$.users[2].name" == "Chloé"
jsonpath "$.users[*].id" includes 2
jsonpath "$.users[?(@.active == true)].name" count == 2
jsonpath "$.users[-1:].email" == "chloe@example.com"
jsonpath "$.users[0:2]" count == 2
jsonpath "$..city" count == 3
jsonpath "$.users[0].address.city" == "Paris"
jsonpath "$.users[2].address.city" == "Αθήνα"
jsonpath "$.users[2].address.zip" == null
jsonpath "$.users[0].roles" count == 2
jsonpath "$.users[2].roles" isEmpty

GET {{origin}}/api/users
[Query]
active: false
HTTP 200
[Asserts]
jsonpath "$.total" == 1
jsonpath "$.users[0].name" == "Bob"

POST {{origin}}/api/users
Content-Type: application/json
{
  "name": "Dan",
  "age": 33,
  "tags": ["new", "test"],
  "nested": {"ok": true}
}
HTTP 201
[Captures]
location: header "Location"
new_id: jsonpath "$.id"
[Asserts]
header "Location" == "/api/users/4"
jsonpath "$.name" == "Dan"
jsonpath "$.tags[1]" == "test"
jsonpath "$.nested.ok" == true

GET {{origin}}{{location}}
HTTP 200
[Asserts]
jsonpath "$.id" == {{new_id}}

PUT {{origin}}/api/users/{{new_id}}
{"name": "Daniel", "age": 34}
HTTP 200
[Asserts]
jsonpath "$.name" == "Daniel"
jsonpath "$.age" == 34

PATCH {{origin}}/api/users/1
{"active": false}
HTTP 200
[Asserts]
jsonpath "$.active" == false
jsonpath "$.name" == "Alice"

DELETE {{origin}}/api/users/4
HTTP 204
[Asserts]
body isEmpty

GET {{origin}}/api/users/99
HTTP 404
[Asserts]
jsonpath "$.error" == "not_found"
jsonpath "$.message" == "user 99 not found"

GET {{origin}}/api/users/2
HTTP 200
ETag: "user-2-v1"
{"id":2,"name":"Bob","email":"bob@example.com","age":25,"active":false,"score":3.25,"roles":["dev"],"address":{"city":"Berlin","zip":"10115"}}

GET {{origin}}/api/types
HTTP 200
[Asserts]
jsonpath "$.int" == 42
jsonpath "$.negative" == -7
jsonpath "$.float" == 3.14159
jsonpath "$.zero" == 0
jsonpath "$.big" == 9007199254740991
jsonpath "$.exp" < 0.001
jsonpath "$.bool" == true
jsonpath "$.falsy" == false
jsonpath "$.nil" == null
jsonpath "$.str" == "héllo wörld ✓"
jsonpath "$.emoji" startsWith "🚀"
jsonpath "$.empty" == ""
jsonpath "$.escaped" == "line1\nline2\t\"quoted\" \\ back"
jsonpath "$.list" count == 5
jsonpath "$.list[1]" == "two"
jsonpath "$.list[2]" == 3
jsonpath "$.list[4]" == null
jsonpath "$.emptyList" count == 0
jsonpath "$.nested.a.b.c[1]" == 20
jsonpath "$.nested..c[2]" == 30
jsonpath "$['key with space']" == "spaced"
jsonpath "$['dash-key']" == "dashed"
jsonpath "$['dot.key']" == "dotted"
jsonpath "$.missing" not exists

GET {{origin}}/api/paged?page=1
HTTP 200
[Captures]
next: jsonpath "$.next"
[Asserts]
jsonpath "$.items[0].id" == 1

GET {{origin}}{{next}}
HTTP 200
[Captures]
next: jsonpath "$.next"
[Asserts]
jsonpath "$.page" == 2
jsonpath "$.items[2].id" == 6

GET {{origin}}{{next}}
HTTP 200
[Asserts]
jsonpath "$.page" == 3
jsonpath "$.next" == null

GET {{origin}}/api/error?status=500
HTTP 500
[Asserts]
jsonpath "$.code" == 500

GET {{origin}}/api/error?status=418
HTTP *
[Asserts]
status == 418
