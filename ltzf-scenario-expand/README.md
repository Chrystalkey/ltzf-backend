# Scenario description Syntax
## Basics
Any Scenario consists of four major elements
```json
{
    "type": "vorgang",
    "context": [],
    "object": {},
    "result": [],
    "shouldfail": false
}
```

Where type gives a hint about which kind of scenario this is, `context` describes the objects to put into the api before, 
`object` is the object that the test is about and `result` contains a list of expected objects within the database. 
`shouldfail` is an optional boolean (default false) and gives an indication of wether this scenario is expected to fail.

## Difference Specification
The objects in `context` and `result` are per default (`{}`) exact copies of `object`. Where they differ, they need to specify in what way.
`object` must be a completely parseable object in terms of the api, but does not have to specifiy all optional fields.
To simplify this we coin the terms `base` for references to the `object`s properties, and `overlay` for the differentially specified object. 

Example:  

```json
{
    "object": {"blub": "xxx", "knast": "brudi"},
    "context": [{
        "blub": "blob"
    }, {}, {"knast": "schwessi"}]
}
```
expands to:  
```json
{
    "object": {"blub": "xxx", "knast": "brudi"},
    "context": [
        {"blub": "blob", "knast": "brudi"},
        {"blub": "xxx", "knast": "brudi"},
        {"blub": "xxx", "knast": "schwessi"}]
}
```

### Arrays
Arrays in objects are specially handled and require a special syntax. Instead of simple 1:1 replacement there are three special rules, 
where references to the base object are index-based.

1. `(overlay) "property": []` replaces `(base) "property": [1,2,3]` (empty list)
2. `null` in overlay arrays specifies that the base property at that point is eliminated and not transferred to the final list. The uneliminated content of base is appended.
   e.g.: `(base) "prop": [1,2,3]` <-> `(overlay) "prop": [null, null, 1]` returns: `"prop": [1,3]`
3. objects in the list are handled recursively `(base) "prop": [{"x": 3, "y": 2}]`, `(overlay) "prop": [{"y": 5}]` will come out as: `"prop": [{"x": 3, "y": 5}]`

Complex case: 
```json
{
    "base": {
        "property": [{"x": 4, "y": 8}, {"x": 1, "y": 2}, {"x": 9, "y": 9}]
    },
    "overlay": {
        "property": [null, {"y": 15}, null, {"x": 15, "y": 23}]
    },
    "result": {
        "property": [{"x": 1, "y": 15}, {"x": 15, "y": 23}]
    }
}
```

Unfortunately, if you want to eliminate all but the last element you will not get around writing `n-1` times `null`. 
If you just want to use the full list, leave out the property entirely.
