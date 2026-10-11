# Captures from every query type, reused in the URL, headers, query string and JSON bodies,
# and variables injected from outside (`--variable`).

GET {{origin}}/api/users/1
HTTP 200
[Captures]
user_id: jsonpath "$.id"
user_name: jsonpath "$.name"
user_score: jsonpath "$.score"
user_active: jsonpath "$.active"
user_roles: jsonpath "$.roles"
first_role: jsonpath "$.roles" nth 0
zip: jsonpath "$.address.zip"
etag: header "ETag"
status_code: status
content_type: header "Content-Type"
body_length: bytes count
name_regex: regex /"name":"(\w+)"/
full_body: body
[Asserts]
variable "user_id" == 1
variable "user_name" == "Alice"
variable "user_score" == 4.5
variable "user_active" == true
variable "user_roles" count == 2
variable "first_role" == "admin"
variable "zip" == "75001"
variable "etag" == "\"user-1-v1\""
variable "status_code" == 200
variable "name_regex" == "Alice"
variable "full_body" startsWith "{\"id\":1"

POST {{origin}}/echo?name={{user_name}}&role={{first_role}}
X-User: {{user_name}}
X-Etag: {{etag}}
{
  "id": {{user_id}},
  "name": "{{user_name}}",
  "active": {{user_active}},
  "score": {{user_score}},
  "length": {{body_length}}
}
HTTP 200
[Captures]
echoed: jsonpath "$.json"
[Asserts]
jsonpath "$.query.name" == "Alice"
jsonpath "$.query.role" == "admin"
jsonpath "$.headers['x-user']" == "Alice"
jsonpath "$.headers['x-etag']" == "\"user-1-v1\""
jsonpath "$.json.id" == 1
jsonpath "$.json.active" == true
jsonpath "$.json.score" == 4.5
jsonpath "$.json.length" == {{body_length}}
variable "echoed" isObject

GET {{origin}}/cookies/set
HTTP 200
[Captures]
theme: cookie "theme"
lang_path: cookie "lang[Path]"
[Asserts]
variable "theme" == "dark"
variable "lang_path" == "/cookies"

GET {{origin}}/html/page
HTTP 200
[Captures]
title: xpath "string(//title)"
li_count: xpath "count(//li)"
version: regex /version: "([^"]+)"/
url: url
[Asserts]
variable "title" == "Compat & Hurl"
variable "li_count" == 3
variable "version" == "1.4.2"
variable "url" endsWith "/html/page"

GET {{origin}}/echo
[Query]
title: {{title}}
count: {{li_count}}
HTTP 200
[Asserts]
jsonpath "$.query.title" == "Compat & Hurl"
jsonpath "$.query.count" == "3.0"

GET {{origin}}/echo
X-Injected: {{str_var}}
HTTP 200
[Asserts]
variable "int_var" == 3
variable "float_var" == 2.5
variable "bool_var" == true
variable "null_var" == null
variable "str_var" == "hello world"
jsonpath "$.headers['x-injected']" == "hello world"
