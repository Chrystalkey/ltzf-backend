echo "Checking for openapi-generator-cli"

if (-Not (Test-Path -Path "oapi-generator" -PathType Container)) {
    Write-Host "Creating oapi-generator directory"
    New-Item -ItemType Directory -Path "oapi-generator" -Force | Out-Null
    Set-Location -Path "oapi-generator"
    
    & Invoke-WebRequest -OutFile openapi-generator-cli.jar https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/7.12.0/openapi-generator-cli-7.12.0.jar
    & Invoke-WebRequest -OutFile openapi.yml raw.githubusercontent.com/Chrystalkey/landtagszusammenfasser/refs/heads/dev-specchange/docs/specs/openapi.yml
    Set-Location -Path ".."
}
if (-Not (Test-Path -Path "oapi-generator/openapi-generator-cli.jar" -PathType Leaf)) {
    Write-Host "Downloading OApi Generator"
    Set-Location -Path "oapi-generator"
    & Invoke-WebRequest -OutFile openapi-generator-cli.jar https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/7.12.0/openapi-generator-cli-7.12.0.jar
    Set-Location -Path ".."
}
if (-Not (Test-Path -Path "oapi-generator/openapi.yml" -PathType Leaf)) {
    Write-Host "Downloading OApi Spec"
    Set-Location -Path "oapi-generator"
    & Invoke-WebRequest -OutFile openapi.yml raw.githubusercontent.com/Chrystalkey/landtagszusammenfasser/refs/heads/dev-specchange/docs/specs/openapi.yml
    Set-Location -Path ".."
}


& java -jar "./oapi-generator/openapi-generator-cli.jar" generate -g rust-axum -i "$(Get-Location)/oapi-generator/openapi.yml" -o "$(Get-Location)/oapicode"
& rg "<I, A, E>" -r "<I, A, E, C>" .\oapicode\src\server\mod.rs --passthrough -N > .\oapicode\src\server\mod1.rs
Get-Content .\oapicode\src\server\mod1.rs | Set-Content -Encoding utf8 .\oapicode\src\server\mod2.rs
& rm .\oapicode\src\server\mod.rs
& rm .\oapicode\src\server\mod1.rs
& mv .\oapicode\src\server\mod2.rs .\oapicode\src\server\mod.rs