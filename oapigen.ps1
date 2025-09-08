echo "Checking for openapi-generator-cli"
$OPENAPI_GENERATOR_VERSION="7.15.0"
$DIRECTORY="oapi-generator"
$SPEC_PATH="raw.githubusercontent.com/Chrystalkey/landtagszusammenfasser/refs/heads/dev-auth/docs/specs/openapi.yml"

if (-Not (Test-Path -Path $DIRECTORY -PathType Container)) {
    Write-Host "Creating oapi-generator directory"
    New-Item -ItemType Directory -Path $DIRECTORY -Force | Out-Null
    Set-Location -Path $DIRECTORY
    
    & Invoke-WebRequest -OutFile openapi-generator-cli.jar https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/$OPENAPI_GENERATOR_VERSION/openapi-generator-cli-$OPENAPI_GENERATOR_VERSION.jar
    & Invoke-WebRequest -OutFile openapi.yml $SPEC_PATH
    Set-Location -Path ".."
}
if (-Not (Test-Path -Path "$DIRECTORY/openapi-generator-cli.jar" -PathType Leaf)) {
    Write-Host "Downloading OApi Generator"
    Set-Location -Path $DIRECTORY
    & Invoke-WebRequest -OutFile openapi-generator-cli.jar https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/$OPENAPI_GENERATOR_VERSION/openapi-generator-cli-$OPENAPI_GENERATOR_VERSION.jar
    Set-Location -Path ".."
}
if (-Not (Test-Path -Path "$DIRECTORY/openapi.yml" -PathType Leaf)) {
    Write-Host "Downloading OApi Spec"
    Set-Location -Path $DIRECTORY
    & Invoke-WebRequest -OutFile openapi.yml $SPEC_PATH
    Set-Location -Path ".."
}


& java -jar "./oapi-generator/openapi-generator-cli.jar" generate -g rust-axum -i "$(Get-Location)/$DIRECTORY/openapi.yml" -o "$(Get-Location)/oapicode"