var details, searchbtn, matchedtxt, svg;
function init(evt) {
    details = document.getElementById("details").firstChild;
    searchbtn = document.getElementById("search");
    matchedtxt = document.getElementById("matched");
    svg = document.getElementsByTagName("svg")[0];
    searching = 0;
}
// mouse-over for info
function s(node) {		// show
    info = g_to_text(node);
    details.nodeValue = nametype + " " + info;
}
function c() {			// clear
    details.nodeValue = ' ';
}
// ctrl-F for search
window.addEventListener("keydown",function (e) {
    if (e.keyCode === 114 || (e.ctrlKey && e.keyCode === 70)) {
        e.preventDefault();
        search_prompt();
    }
})
// functions
function find_child(parent, name, attr) {
    var children = parent.childNodes;
    for (var i=0; i<children.length;i++) {
        if (children[i].tagName == name)
            return (attr != undefined) ? children[i].attributes[attr].value : children[i];
    }
    return;
}
function orig_save(e, attr, val) {
    if (e.attributes["_orig_"+attr] != undefined) return;
    if (e.attributes[attr] == undefined) return;
    if (val == undefined) val = e.attributes[attr].value;
    e.setAttribute("_orig_"+attr, val);
}
function orig_load(e, attr) {
    if (e.attributes["_orig_"+attr] == undefined) return;
    e.attributes[attr].value = e.attributes["_orig_"+attr].value;
    e.removeAttribute("_orig_"+attr);
}
function g_to_text(e) {
    var text = find_child(e, "title").firstChild.nodeValue;
    return (text)
}
function g_to_func(e) {
    var func = g_to_text(e);
    // if there's any manipulation we want to do to the function
    // name before it's searched, do it here before returning.
    return (func);
}
function update_text(e) {
    var r = find_child(e, "rect");
    var t = find_child(e, "text");
    var w = parseFloat(r.attributes["width"].value) -3;
    var txt = find_child(e, "title").textContent.replace(/\\([^(]*\\)\$/,"");
    t.attributes["x"].value = parseFloat(r.attributes["x"].value) +3;
    // Smaller than this size won't fit anything
    if (w < 2*fontsize*fontwidth) {
        t.textContent = "";
        return;
    }
    t.textContent = txt;
    // Fit in full text width
    if (/^ *\$/.test(txt) || t.getSubStringLength(0, txt.length) < w)
        return;
    for (var x=txt.length-2; x>0; x--) {
        if (t.getSubStringLength(0, x+2) <= w) {
            t.textContent = txt.substring(0,x) + "..";
            return;
        }
    }
    t.textContent = "";
}
// zoom
function zoom_reset(e) {
    if (e.attributes != undefined) {
        orig_load(e, "x");
        orig_load(e, "width");
    }
    if (e.childNodes == undefined) return;
    for(var i=0, c=e.childNodes; i<c.length; i++) {
        zoom_reset(c[i]);
    }
}
function zoom_child(e, x, ratio) {
    if (e.attributes != undefined) {
        if (e.attributes["x"] != undefined) {
            orig_save(e, "x");
            e.attributes["x"].value = (parseFloat(e.attributes["x"].value) - x - xpad) * ratio + xpad;
            if(e.tagName == "text") e.attributes["x"].value = find_child(e.parentNode, "rect", "x") + 3;
        }
        if (e.attributes["width"] != undefined) {
            orig_save(e, "width");
            e.attributes["width"].value = parseFloat(e.attributes["width"].value) * ratio;
        }
    }
    if (e.childNodes == undefined) return;
    for(var i=0, c=e.childNodes; i<c.length; i++) {
        zoom_child(c[i], x-xpad, ratio);
    }
}
function zoom_parent(e) {
    if (e.attributes) {
        if (e.attributes["x"] != undefined) {
            orig_save(e, "x");
            e.attributes["x"].value = xpad;
        }
        if (e.attributes["width"] != undefined) {
            orig_save(e, "width");
            e.attributes["width"].value = parseInt(svg.width.baseVal.value) - (xpad*2);
        }
    }
    if (e.childNodes == undefined) return;
    for(var i=0, c=e.childNodes; i<c.length; i++) {
        zoom_parent(c[i]);
    }
}
function zoom(node) {
    var attr = find_child(node, "rect").attributes;
    var width = parseFloat(attr["width"].value);
    var xmin = parseFloat(attr["x"].value);
    var xmax = parseFloat(xmin + width);
    var ymin = parseFloat(attr["y"].value);
    var ratio = (svg.width.baseVal.value - 2*xpad) / width;
    // XXX: Workaround for JavaScript float issues (fix me)
    var fudge = 0.0001;
    var unzoombtn = document.getElementById("unzoom");
    unzoombtn.style["opacity"] = "1.0";
    var el = document.getElementsByTagName("g");
    for(var i=0;i<el.length;i++){
        var e = el[i];
        var a = find_child(e, "rect").attributes;
        var ex = parseFloat(a["x"].value);
        var ew = parseFloat(a["width"].value);
        // Is it an ancestor
        if (!inverted) {
            var upstack = parseFloat(a["y"].value) > ymin;
        } else {
            var upstack = parseFloat(a["y"].value) < ymin;
        }
        if (upstack) {
            // Direct ancestor
            if (ex <= xmin && (ex+ew+fudge) >= xmax) {
                e.style["opacity"] = "0.5";
                zoom_parent(e);
                e.onclick = function(e){unzoom(); zoom(this);};
                update_text(e);
            }
            // not in current path
            else
                e.style["display"] = "none";
        }
        // Children maybe
        else {
            // no common path
            if (ex < xmin || ex + fudge >= xmax) {
                e.style["display"] = "none";
            }
            else {
                zoom_child(e, xmin, ratio);
                e.onclick = function(e){zoom(this);};
                update_text(e);
            }
        }
    }
}
function unzoom() {
    var unzoombtn = document.getElementById("unzoom");
    unzoombtn.style["opacity"] = "0.0";
    var el = document.getElementsByTagName("g");
    for(i=0;i<el.length;i++) {
        el[i].style["display"] = "block";
        el[i].style["opacity"] = "1";
        zoom_reset(el[i]);
        update_text(el[i]);
    }
}
// search
function reset_search() {
    var el = document.getElementsByTagName("rect");
    for (var i=0; i < el.length; i++) {
        orig_load(el[i], "fill")
    }
}
function search_prompt() {
    if (!searching) {
        var term = prompt("Enter a search term (regexp " +
            "allowed, eg: ^ext4_)", "");
        if (term != null) {
            search(term)
        }
    } else {
        reset_search();
        searching = 0;
        searchbtn.style["opacity"] = "0.1";
        searchbtn.firstChild.nodeValue = "Search"
        matchedtxt.style["opacity"] = "0.0";
        matchedtxt.firstChild.nodeValue = ""
    }
}
function search(term) {
    var re = new RegExp(term);
    var el = document.getElementsByTagName("g");
    var matches = new Object();
    var maxwidth = 0;
    for (var i = 0; i < el.length; i++) {
        var e = el[i];
        if (e.attributes["class"].value != "func_g")
            continue;
        var func = g_to_func(e);
        var rect = find_child(e, "rect");
        if (rect == null) {
            // the rect might be wrapped in an anchor
            // if nameattr href is being used
            if (rect = find_child(e, "a")) {
                rect = find_child(r, "rect");
            }
        }
        if (func == null || rect == null)
            continue;
        // Save max width. Only works as we have a root frame
        var w = parseFloat(rect.attributes["width"].value);
        if (w > maxwidth)
            maxwidth = w;
        if (func.match(re)) {
            // highlight
            var x = parseFloat(rect.attributes["x"].value);
            orig_save(rect, "fill");
            rect.attributes["fill"].value = searchcolor;
            // remember matches
            if (matches[x] == undefined) {
                matches[x] = w;
            } else {
                if (w > matches[x]) {
                    // overwrite with parent
                    matches[x] = w;
                }
            }
            searching = 1;
        }
    }
    if (!searching)
        return;
    searchbtn.style["opacity"] = "1.0";
    searchbtn.firstChild.nodeValue = "Reset Search"
    // calculate percent matched, excluding vertical overlap
    var count = 0;
    var lastx = -1;
    var lastw = 0;
    var keys = Array();
    for (k in matches) {
        if (matches.hasOwnProperty(k))
            keys.push(k);
    }
    // sort the matched frames by their x location
    // ascending, then width descending
    keys.sort(function(a, b){
        return a - b;
    });
    // Step through frames saving only the biggest bottom-up frames
    // thanks to the sort order. This relies on the tree property
    // where children are always smaller than their parents.
    var fudge = 0.0001;	// JavaScript floating point
    for (var k in keys) {
        var x = parseFloat(keys[k]);
        var w = matches[keys[k]];
        if (x >= lastx + lastw - fudge) {
            count += w;
            lastx = x;
            lastw = w;
        }
    }
    // display matched percent
    matchedtxt.style["opacity"] = "1.0";
    pct = 100 * count / maxwidth;
    if (pct == 100)
        pct = "100"
    else
        pct = pct.toFixed(1)
    matchedtxt.firstChild.nodeValue = "Matched: " + pct + "%";
}
function searchover(e) {
    searchbtn.style["opacity"] = "1.0";
}
function searchout(e) {
    if (searching) {
        searchbtn.style["opacity"] = "1.0";
    } else {
        searchbtn.style["opacity"] = "0.1";
    }
}
