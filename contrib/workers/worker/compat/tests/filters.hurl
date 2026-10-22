# Filters: encodings, conversions, strings, lists, dates, nested JSONPath/XPath, URLs.

GET {{origin}}/api/types
HTTP 200
[Asserts]
jsonpath "$.base64" base64Decode utf8Decode == "Hello Hurl"
jsonpath "$.base64" base64Decode toHex == "48656c6c6f204875726c"
jsonpath "$.base64" base64Decode == hex,48656c6c6f204875726c;
jsonpath "$.str" utf8Encode base64Encode base64Decode utf8Decode == "héllo wörld ✓"
jsonpath "$.str" utf8Encode base64UrlSafeEncode base64UrlSafeDecode utf8Decode == "héllo wörld ✓"
jsonpath "$.str" utf8Encode toHex == "68c3a96c6c6f2077c3b6726c6420e29c93"
jsonpath "$.str" urlEncode == "h%C3%A9llo%20w%C3%B6rld%20%E2%9C%93"
jsonpath "$.str" urlEncode urlDecode == "héllo wörld ✓"
jsonpath "$.escaped" htmlEscape contains "&quot;quoted&quot;"
jsonpath "$.escaped" htmlEscape htmlUnescape == "line1\nline2\t\"quoted\" \\ back"
jsonpath "$.int" toString == "42"
jsonpath "$.float" toString == "3.14159"
jsonpath "$.int" toFloat == 42.0
jsonpath "$.list[1]" replace "two" "2" toInt == 2
jsonpath "$.str" replace "wörld" "world" == "héllo world ✓"
jsonpath "$.escaped" replaceRegex /\s+/ "_" == "line1_line2_\"quoted\"_\\_back"
jsonpath "$.date" regex /(\d{4})-/ == "2026"
jsonpath "$.date" split "T" nth 0 == "2026-01-15"
jsonpath "$.ipv4" split "." count == 4
jsonpath "$.ipv4" split "." last toInt == 10
jsonpath "$.ipv4" split "." first == "192"
jsonpath "$.uuid" split "-" nth 4 == "1a2b3c4d5e6f"
jsonpath "$.list" first == 1
jsonpath "$.list" last == null
jsonpath "$.list" nth 1 == "two"
jsonpath "$.list" nth -1 == null
jsonpath "$.list" count == 5
jsonpath "$.date" toDate "%Y-%m-%dT%H:%M:%SZ" dateFormat "%Y/%m/%d %H:%M" == "2026/01/15 10:20"
jsonpath "$.date" toDate "%Y-%m-%dT%H:%M:%SZ" dateFormat "%A %B" == "Thursday January"
jsonpath "$.date" toDate "%Y-%m-%dT%H:%M:%SZ" daysBeforeNow > 0
jsonpath "$.date" toDate "%Y-%m-%dT%H:%M:%SZ" daysAfterNow < 0
header "Content-Type" replace "json" "xml" == "application/xml"
bytes charsetDecode "utf-8" contains "héllo"
body jsonpath "$.nested.a.b.c[0]" == 10

GET {{origin}}/text/latin1
HTTP 200
[Asserts]
bytes charsetDecode "iso-8859-1" == "café crème"
bytes toHex == "636166e9206372e86d65"

GET {{origin}}/html/page
HTTP 200
[Asserts]
body xpath "string(//title)" == "Compat & Hurl"
body xpath "count(//li)" == 3
xpath "string(//title)" htmlEscape == "Compat &amp; Hurl"
regex /csrf" value="([^"]+)"/ split "-" nth 1 toInt == 31415926

GET {{origin}}/api/paged?page=2
HTTP 200
[Asserts]
url urlQueryParam "page" == "2"

GET {{origin}}/redirect/2
[Options]
location: true
HTTP 200
[Asserts]
redirects count == 2
redirects nth 0 location endsWith "/redirect/1"
redirects nth 1 location endsWith "/redirect/0"
