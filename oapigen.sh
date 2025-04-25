#!/bin/bash

export OPENAPI_GENERATOR_VERSION="7.12.0"
DIRECTORY="oapi-generator"

echo "Checking for openapi-generator-cli"

# Create oapi-generator directory if it doesn't exist
if [ ! -d "oapi-generator" ]; then
    echo "Creating oapi-generator directory"
    mkdir oapi-generator
    cd oapi-generator || exit

    curl -o openapi-generator-cli.jar https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/7.12.0/openapi-generator-cli-7.12.0.jar
    curl -o openapi.yml https://raw.githubusercontent.com/Chrystalkey/landtagszusammenfasser/refs/heads/dev-specchange/docs/specs/openapi.yml

    cd ..
fi

# Download JAR if missing
if [ ! -f "oapi-generator/openapi-generator-cli.jar" ]; then
    echo "Downloading OApi Generator"
    cd oapi-generator || exit
    curl -o openapi-generator-cli.jar https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/7.12.0/openapi-generator-cli-7.12.0.jar
    cd ..
fi

# Download YAML spec if missing
if [ ! -f "oapi-generator/openapi.yml" ]; then
    echo "Downloading OApi Spec"
    cd oapi-generator || exit
    curl -o openapi.yml https://raw.githubusercontent.com/Chrystalkey/landtagszusammenfasser/refs/heads/dev-specchange/docs/specs/openapi.yml
    cd ..
fi

# Run openapi-generator
OPENAPI_GENERATOR_VERSION="7.12.0" ./$DIRECTORY/openapi-generator-cli generate -g rust-axum -i $DIRECTORY/openapi.yml -o $(pwd)/oapicode

# Replace <I, A, E> with <I, A, E, C> in mod.rs using ripgrep and create mod2.rs
rg '<I, A, E>' -r '<I, A, E, C>' ./oapicode/src/server/mod.rs --passthrough -N > ./oapicode/src/server/mod1.rs

# Convert to UTF-8 and move the file
iconv -t UTF-8 ./oapicode/src/server/mod1.rs -o ./oapicode/src/server/mod2.rs

# Replace mod.rs with mod2.rs
rm ./oapicode/src/server/mod.rs
rm ./oapicode/src/server/mod1.rs
mv ./oapicode/src/server/mod2.rs ./oapicode/src/server/mod.rs
