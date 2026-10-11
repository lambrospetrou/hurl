# HTML pages, JavaScript and CSS assets, a form login and an Atom feed (XPath through libxml2).

GET {{origin}}/html/page
HTTP 200
[Captures]
csrf: xpath "string(//input[@name='csrf']/@value)" redact
js_path: xpath "string(//script[@src]/@src)"
css_path: xpath "string(//link[@rel='stylesheet']/@href)"
[Asserts]
header "Content-Type" contains "text/html"
body startsWith "<!DOCTYPE html>"
xpath "string(//title)" == "Compat & Hurl"
xpath "string(/html/@lang)" == "en"
xpath "string(//meta[@name='description']/@content)" == "Compatibility page"
xpath "count(//nav/a)" == 3
xpath "string(//a[@class='active']/@href)" == "/about"
xpath "string(//nav/a[3]/@href)" == "https://example.com/x?a=1&b=2"
xpath "normalize-space(//h1)" == "Welcome, Hurl"
xpath "string(//h1/@id)" == "title"
xpath "count(//p)" == 3
xpath "string(//p[@class='note']/b)" == "bold"
xpath "count(//ul/li)" == 3
xpath "string(//li[2])" == "Two"
xpath "count(//table[@id='prices']/tr)" == 3
xpath "count(//table//td)" == 4
xpath "string(//table/tr[3]/td[1])" == "Pear"
xpath "sum(//table/tr/td[2])" == 3.75
xpath "count(//form//input)" == 3
xpath "boolean(//form[@method='post'])" == true
xpath "boolean(//video)" == false
xpath "count(//comment())" == 1
xpath "contains(string(//script[not(@src)]), 'window.APP')" == true
xpath "normalize-space(//p[last()])" == "Café <tag> 🚀"
xpath "string(//body/@class)" == "home"
xpath "//nav/a" count == 3
regex /version: "([^"]+)"/ == "1.4.2"
body contains "console.log(\"</div>\")"

GET {{origin}}{{js_path}}
HTTP 200
[Asserts]
header "Content-Type" startsWith "application/javascript"
body contains "export function greet(name)"
regex /version: "([^"]+)"/ == "1.4.2"
regex /retries:\s*(\d+)/ toInt == 3
body matches /const API_BASE = "\/api\/v\d+";/
body contains "${name}"

GET {{origin}}{{css_path}}
HTTP 200
[Asserts]
header "Content-Type" == "text/css"
body contains ".note { font-weight: bold; }"

POST {{origin}}/html/login
[Options]
location: true
[Form]
csrf: {{csrf}}
user: alice
password: pw
HTTP 200
[Asserts]
url endsWith "/html/dashboard"
redirects count == 1
xpath "string(//h1)" == "Dashboard"
xpath "string(//p[@class='user'])" == "alice"
body xpath "count(//p)" == 1

GET {{origin}}/html/dashboard
HTTP 200
[Asserts]
cookie "web_session" not exists
xpath "string(//h1)" == "Dashboard"

GET {{origin}}/xml/feed
HTTP 200
[Asserts]
xpath "string(//_:feed/_:title)" == "Compat feed"
xpath "count(//_:entry)" == 2
xpath "string(//_:entry[2]/_:title)" == "Second & last"
xpath "string(//media:thumbnail/@url)" == "https://example.com/1.png"
xpath "count(//*[local-name()='entry'])" == 2
