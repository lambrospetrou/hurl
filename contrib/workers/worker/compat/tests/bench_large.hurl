# Benchmark: large JSON and HTML bodies, many JSONPath/XPath queries and repeated requests.

GET {{origin}}/api/large?n=20000
HTTP 200
[Captures]
last_name: jsonpath "$.items[19999].name"
[Asserts]
jsonpath "$.count" == 20000
jsonpath "$.items" count == 20000
jsonpath "$.items[?(@.in_stock == false)]" count == 6667
jsonpath "$.items[?(@.price > 90)].id" count == 2000
jsonpath "$..tags[1]" count == 20000
jsonpath "$.items[19999].name" == "item-19999"
variable "last_name" == "item-19999"

GET {{origin}}/html/large?rows=10000
HTTP 200
[Asserts]
xpath "count(//tr)" == 10000
xpath "count(//tr[@class='odd'])" == 5000
xpath "string(//tr[last()]/td[2])" == "name-9999"
xpath "sum(//tr/td[1])" == 49995000

GET {{origin}}/api/users
[Options]
repeat: 50
HTTP 200
[Asserts]
jsonpath "$.users" count == 3
