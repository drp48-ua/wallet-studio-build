param([string]$ManifestPath='dist/release-manifest.json')
$ErrorActionPreference='Stop'
function ConvertTo-Base64Url([byte[]]$Bytes) { [Convert]::ToBase64String($Bytes).TrimEnd('=').Replace('+','-').Replace('/','_') }
function New-DriveAssertion($Credential) {
    if($Credential.type -cne 'service_account' -or $Credential.client_email -cnotmatch '^[^@\s]+@[^@\s]+\.iam\.gserviceaccount\.com$' -or $Credential.token_uri -cne 'https://oauth2.googleapis.com/token' -or $Credential.subject -or $Credential.sub){throw 'Invalid non-delegated service account configuration'}
    $now=[DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $header=ConvertTo-Base64Url ([Text.Encoding]::UTF8.GetBytes('{"alg":"RS256","typ":"JWT"}'))
    $claims=@{iss=$Credential.client_email;scope='https://www.googleapis.com/auth/drive';aud='https://oauth2.googleapis.com/token';iat=$now;exp=$now+600}|ConvertTo-Json -Compress
    $inputText=$header+'.'+(ConvertTo-Base64Url ([Text.Encoding]::UTF8.GetBytes($claims)))
    $key=[Security.Cryptography.RSA]::Create()
    try{$key.ImportFromPem($Credential.private_key);$signature=$key.SignData([Text.Encoding]::UTF8.GetBytes($inputText),[Security.Cryptography.HashAlgorithmName]::SHA256,[Security.Cryptography.RSASignaturePadding]::Pkcs1)}finally{$key.Dispose()}
    $inputText+'.'+(ConvertTo-Base64Url $signature)
}
function Send-DriveHttp($Request) {
    $handler=[Net.Http.HttpClientHandler]::new();$handler.AllowAutoRedirect=$false
    $client=[Net.Http.HttpClient]::new($handler);$client.Timeout=[TimeSpan]::FromMinutes(10)
    $message=[Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::new($Request.method),$Request.url)
    try {
        if($Request.file){$message.Content=[Net.Http.StreamContent]::new([IO.File]::OpenRead($Request.file))}
        elseif($null -ne $Request.body){$message.Content=[Net.Http.ByteArrayContent]::new([Text.Encoding]::UTF8.GetBytes($Request.body))}
        foreach($name in $Request.headers.Keys){if(-not $message.Headers.TryAddWithoutValidation($name,[string]$Request.headers[$name])){$message.Content.Headers.TryAddWithoutValidation($name,[string]$Request.headers[$name])|Out-Null}}
        $response=$client.SendAsync($message,[Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
        try {
            $stream=$response.Content.ReadAsStream();$buffer=[byte[]]::new(65537);$length=0
            while($length -lt $buffer.Length){$read=$stream.Read($buffer,$length,$buffer.Length-$length);if(-not $read){break};$length+=$read}
            if($length -gt 65536){throw 'Oversized API response'}
            $text=[Text.Encoding]::UTF8.GetString($buffer,0,$length);$json=if($text){$text|ConvertFrom-Json}else{$null}
            $headers=@{};foreach($entry in $response.Headers){$headers[$entry.Key]=($entry.Value -join ',')}
            @{status=[int]$response.StatusCode;headers=$headers;json=$json}
        }finally{$response.Dispose()}
    }catch{throw 'Drive HTTP operation failed; response details suppressed'}finally{$message.Dispose();$client.Dispose();$handler.Dispose()}
}
function Invoke-DriveFileSync($Manifest,[string]$Zip,[string]$FileId,$Credential,[scriptblock]$Http,[scriptblock]$ValidateChannel,[string]$InitialSha256) {
    if($FileId -cnotmatch '^[A-Za-z0-9_-]{20,128}$'){throw 'Invalid approved Drive file ID'}
    & $ValidateChannel
    $assertion=New-DriveAssertion $Credential
    $tokenReply=& $Http @{method='POST';url='https://oauth2.googleapis.com/token';headers=@{'Content-Type'='application/x-www-form-urlencoded'};body=('grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion='+[Uri]::EscapeDataString($assertion))}
    $assertion=$null
    if($tokenReply.status -ne 200 -or -not $tokenReply.json.access_token -or $tokenReply.json.token_type -cne 'Bearer'){throw 'Service account token exchange rejected'}
    $auth=@{Authorization='Bearer '+$tokenReply.json.access_token};$tokenReply=$null
    $fields='id,name,mimeType,size,md5Checksum,sha256Checksum,appProperties,version,trashed,capabilities(canEdit)'
    $metadataUrl="https://www.googleapis.com/drive/v3/files/${FileId}?fields=$fields"
    $before=& $Http @{method='GET';url=$metadataUrl;headers=$auth}
    if($before.status -ne 200 -or $before.json.id -cne $FileId -or $before.json.trashed -or $before.json.mimeType -cne 'application/zip' -or -not $before.json.capabilities.canEdit -or -not $before.headers.ETag -or $before.json.version -cnotmatch '^\d+$' -or -not $before.json.name){throw 'Existing editable ZIP/conditional ownership not proven'}
    $oldBuild=$before.json.appProperties.wallet_studio_build_id
    # First enrollment must bind the exact already-reviewed public package;
    # missing appProperties never authorize replacing an unknown newer file.
    if(-not $oldBuild -and ($InitialSha256 -cnotmatch '^[0-9a-f]{64}$' -or $before.json.sha256Checksum -cne $InitialSha256)){throw 'Unenrolled Drive bytes require an explicit reviewed initial SHA256'}
    if($oldBuild -and ($oldBuild -cnotmatch '^\d+$' -or [long]$oldBuild -gt [long]$Manifest.build_id)){throw 'Drive holds a newer or unknown build; refuse rollback'}
    $sha=(Get-FileHash -LiteralPath $Zip -Algorithm SHA256).Hash.ToLowerInvariant();$size=(Get-Item -LiteralPath $Zip).Length
    if($sha -cne $Manifest.sha256 -or $size -ne $Manifest.size){throw 'Verified ZIP changed before Drive sync'}
    if($oldBuild -and [long]$oldBuild -eq [long]$Manifest.build_id){
        if($before.json.sha256Checksum -ceq $sha -and [long]$before.json.size -eq $size){Write-Output 'Drive already contains this exact verified build';return}
        throw 'Equal Drive build has different/unproven bytes'
    }
    $metadata=@{appProperties=@{wallet_studio_build_id=[string]$Manifest.build_id;wallet_studio_commit=$Manifest.commit;wallet_studio_sha256=$sha}}|ConvertTo-Json -Compress
    $headers=@{Authorization=$auth.Authorization;'Content-Type'='application/json; charset=UTF-8';'X-Upload-Content-Type'='application/zip';'X-Upload-Content-Length'=[string]$size;'If-Match'=$before.headers.ETag}
    & $ValidateChannel
    $start=& $Http @{method='PATCH';url="https://www.googleapis.com/upload/drive/v3/files/${FileId}?uploadType=resumable&fields=$fields";headers=$headers;body=$metadata}
    $upload=$null
    if($start.status -ne 200 -or -not [Uri]::TryCreate([string]$start.headers.Location,[UriKind]::Absolute,[ref]$upload) -or $upload.Scheme -cne 'https' -or $upload.Host -cne 'www.googleapis.com' -or -not $upload.IsDefaultPort -or $upload.UserInfo -or $upload.Fragment -or $upload.AbsolutePath -cne "/upload/drive/v3/files/$FileId"){throw 'Drive did not return a trusted resumable session'}
    $current=& $Http @{method='GET';url=$metadataUrl;headers=$auth}
    if($current.status -ne 200 -or $current.json.version -cne $before.json.version -or $current.headers.ETag -cne $before.headers.ETag){throw 'Drive file changed before upload; session abandoned'}
    & $ValidateChannel
    $sent=& $Http @{method='PUT';url=$upload.AbsoluteUri;headers=@{Authorization=$auth.Authorization;'Content-Type'='application/zip';'If-Match'=$before.headers.ETag};file=$Zip}
    if($sent.status -ne 200){throw 'Drive upload did not complete; inspect delivery before retry'}
    $after=& $Http @{method='GET';url=$metadataUrl;headers=$auth}
    $md5=(Get-FileHash -LiteralPath $Zip -Algorithm MD5).Hash.ToLowerInvariant()
    if($after.status -ne 200 -or $after.json.id -cne $FileId -or $after.json.name -cne $before.json.name -or [long]$after.json.size -ne $size -or $after.json.md5Checksum -cne $md5 -or ($after.json.sha256Checksum -and $after.json.sha256Checksum -cne $sha) -or $after.json.appProperties.wallet_studio_build_id -ne [string]$Manifest.build_id){throw 'Drive readback integrity mismatch; delivery requires review'}
    Write-Output 'Existing Drive file synchronized and checksum verified; ID/name/permissions unchanged by this script'
}
if($MyInvocation.InvocationName -ne '.') {
    if($env:WALLET_STUDIO_DRIVE_SYNC_ENABLED -cne 'true'){Write-Output 'Drive sync disabled; no synchronization claimed';return}
    if(-not $env:WALLET_STUDIO_DRIVE_SERVICE_ACCOUNT_JSON -or -not $env:WALLET_STUDIO_DRIVE_FILE_ID){throw 'Drive sync enabled without approved credentials/file ID'}
    try {
        $zip=& (Join-Path $PSScriptRoot 'prepare-release-alias.ps1') -ManifestPath $ManifestPath
        $manifest=Get-Content -LiteralPath $ManifestPath -Raw|ConvertFrom-Json
        if($env:GITHUB_REF -cne 'refs/heads/main' -or $manifest.commit -cne $env:GITHUB_SHA -or $manifest.repository -cne $env:GITHUB_REPOSITORY -or $manifest.build_id -ne [long]$env:GITHUB_RUN_NUMBER){throw 'Source binding rejected'}
        $channel={
            $head=gh api "repos/$env:GITHUB_REPOSITORY/commits/main" --jq .sha
            if($LASTEXITCODE -ne 0 -or $head -cne $env:GITHUB_SHA){throw 'Current main changed'}
            $latest=gh api "repos/$env:GITHUB_REPOSITORY/releases/latest" --jq .tag_name
            if($LASTEXITCODE -ne 0 -or $latest -cne "desktop-build-$env:GITHUB_RUN_NUMBER"){throw 'Latest published build changed'}
        }
        $credential=$env:WALLET_STUDIO_DRIVE_SERVICE_ACCOUNT_JSON|ConvertFrom-Json
        Invoke-DriveFileSync $manifest $zip $env:WALLET_STUDIO_DRIVE_FILE_ID $credential ${function:Send-DriveHttp} $channel $env:WALLET_STUDIO_DRIVE_INITIAL_SHA256
    }catch{throw 'Drive delivery sync failed; secrets and HTTP details suppressed. Existing file/delivery needs review.'}
}
