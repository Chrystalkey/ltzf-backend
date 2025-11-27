diff --suppress-common-lines -y <(jq '.actual' $1 | jq 'walk(if type == "array" then sort else . end)')  \
     <(jq '.expected'  $1 | jq 'walk(if type == "array" then sort else . end)')
