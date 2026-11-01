.pragma library
// Rendered-markdown cache shared by all chat bubbles.
//
// Kept in a library script on purpose: state held in QML properties would be
// read inside each bubble's text binding, so every insert would re-evaluate
// every bubble. Library variables are not tracked by bindings.
//
// Streaming answers produce many intermediate texts, so the whole cache is
// dropped once it reaches MAX entries instead of tracking recency.

var MAX = 400
var map = {}
var size = 0

// Cached HTML for `text`, or undefined.
function get(text) {
    return map[text]
}

function put(text, html) {
    if (size >= MAX) {
        map = {}
        size = 0
    }
    if (map[text] === undefined)
        size++
    map[text] = html
}
