#!/bin/bash

export OPENAPI_GENERATOR_VERSION="7.14.0"
DIRECTORY="oapi-generator"

echo "Checking for openapi-generator-cli"

# Create oapi-generator directory if it doesn't exist
if [ ! -d "oapi-generator" ]; then
    echo "Creating oapi-generator directory"
    mkdir oapi-generator
    cd oapi-generator || exit

    curl -o openapi-generator-cli.jar https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/$OPENAPI_GENERATOR_VERSION/openapi-generator-cli-$OPENAPI_GENERATOR_VERSION.jar
    curl -o openapi.yml https://raw.githubusercontent.com/Chrystalkey/landtagszusammenfasser/refs/heads/main/docs/specs/openapi.yml

    cd ..
fi

# Download JAR if missing
if [ ! -f "oapi-generator/openapi-generator-cli.jar" ]; then
    echo "Downloading OApi Generator"
    cd oapi-generator || exit
    curl -o openapi-generator-cli.jar https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/$OPENAPI_GENERATOR_VERSION/openapi-generator-cli-$OPENAPI_GENERATOR_VERSION.jar
    cd ..
fi

# Download YAML spec if missing
if [ ! -f "oapi-generator/openapi.yml" ]; then
    echo "Downloading OApi Spec"
    cd oapi-generator || exit
    curl -o openapi.yml https://raw.githubusercontent.com/Chrystalkey/landtagszusammenfasser/refs/heads/main/docs/specs/openapi.yml
    cd ..
fi

# Run openapi-generator
java -jar ./$DIRECTORY/openapi-generator-cli.jar generate -g rust-axum -i $DIRECTORY/openapi.yml -o $(pwd)/oapicode
