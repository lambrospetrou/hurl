# Every predicate, on JSON, text and headers, with and without `not`, and implicit asserts.

GET {{origin}}/api/types
HTTP/1.1 200
Content-Type: application/json
[Asserts]
status == 200
status >= 200
status < 300
status != 404
version == "1.1"
duration < 10000
header "Content-Type" == "application/json"
header "Content-Type" startsWith "application/"
header "Content-Type" endsWith "json"
header "Content-Type" contains "json"
header "Content-Type" matches /^application\/(json|xml)$/
header "Content-Type" matches "^application/.*$"
jsonpath "$.int" == 42
jsonpath "$.int" != 41
jsonpath "$.int" > 41
jsonpath "$.int" >= 42
jsonpath "$.int" < 43
jsonpath "$.int" <= 42
jsonpath "$.float" > 3.14
jsonpath "$.float" < 3.15
jsonpath "$.negative" < 0
jsonpath "$.int" isInteger
jsonpath "$.int" isNumber
jsonpath "$.float" isFloat
jsonpath "$.float" isNumber
jsonpath "$.float" not isInteger
jsonpath "$.bool" isBoolean
jsonpath "$.str" isString
jsonpath "$.list" isCollection
jsonpath "$.list" isList
jsonpath "$.emptyObj" isObject
jsonpath "$.emptyObj" isCollection
jsonpath "$.emptyObj" isEmpty
jsonpath "$.emptyList" isEmpty
jsonpath "$.empty" isEmpty
jsonpath "$.str" not isEmpty
jsonpath "$.list" not isEmpty
jsonpath "$.list" includes "two"
jsonpath "$.list" includes 1
jsonpath "$.list" includes true
jsonpath "$.list" includes null
jsonpath "$.list" not includes "three"
jsonpath "$.date" isIsoDate
jsonpath "$.date" toDate "%Y-%m-%dT%H:%M:%SZ" isDate
jsonpath "$.str" not isIsoDate
jsonpath "$.uuid" isUuid
jsonpath "$.str" not isUuid
jsonpath "$.ipv4" isIpv4
jsonpath "$.ipv4" not isIpv6
jsonpath "$.ipv6" isIpv6
jsonpath "$.ipv6" not isIpv4
jsonpath "$.str" startsWith "héllo"
jsonpath "$.str" endsWith "✓"
jsonpath "$.str" contains "wörld"
jsonpath "$.str" matches /^h.llo w.rld .$/
jsonpath "$.str" not matches /^\d+$/
jsonpath "$.nil" == null
jsonpath "$.nil" exists
jsonpath "$.nope" not exists
jsonpath "$.nil" not isString
jsonpath "$.big" == 9007199254740991
jsonpath "$.exp" == 0.00000015
body contains "\"int\":42"
bytes count > 100
bytes contains hex,22696e74223a3432;

GET {{origin}}/text/plain
HTTP 200
[Asserts]
body == "Hello World!\nSecond line\n"
body != "Hello"
body contains "World"
body not contains "Goodbye"
body startsWith "Hello"
body not startsWith "World"
body endsWith "\n"
body matches /Second\s+line/
bytes startsWith hex,48656c6c6f;
bytes endsWith hex,0a;
bytes == hex,48656c6c6f20576f726c64210a5365636f6e64206c696e650a;
sha256 == hex,5b64c613be805887b60007a85a6e255950d27ee9e272e554f7c7a94978342d0d;
md5 == hex,02005d8f0084f55274a6f5e3dc7eae5d;
