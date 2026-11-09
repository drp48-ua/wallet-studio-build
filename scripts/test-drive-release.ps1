$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'sync-drive-release.ps1')
$testRoot=Join-Path ([IO.Path]::GetTempPath()) ('wallet-drive-test-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot|Out-Null
$key=[Security.Cryptography.RSA]::Create(2048)
try {
    $zip=Join-Path $testRoot 'public.zip';[IO.File]::WriteAllText($zip,'synthetic public ZIP bytes')
    $manifest=@{build_id=5;commit='synthetic-commit';sha256=(Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant();size=(Get-Item $zip).Length}
    $credential=@{type='service_account';client_email='fixture@synthetic.iam.gserviceaccount.com';token_uri='https://oauth2.googleapis.com/token';private_key=$key.ExportPkcs8PrivateKeyPem()}
    $id='synthetic_existing_file_123456789'
    $passed=0
    foreach($case in @('valid','newer-build','wrong-id','missing-etag','unsafe-session','changed-version','main-changed','token-rejected','upload-failed','bad-readback','invalid-id','bootstrap-reviewed','bootstrap-unknown')) {
        $state=@{case=$case;requests=[Collections.Generic.List[object]]::new();checks=0;uploaded=$false;gets=0}
        $http={param($request)
            $state.requests.Add($request)
            if($request.method -eq 'POST') {
                $encoded=($request.body -split 'assertion=',2)[1];$jwt=[Uri]::UnescapeDataString($encoded);$part=($jwt -split '\.')[1].Replace('-','+').Replace('_','/');$part=$part.PadRight($part.Length+(4-$part.Length%4)%4,'=')
                $claims=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($part))|ConvertFrom-Json
                if($claims.sub -or $claims.scope -cne 'https://www.googleapis.com/auth/drive' -or $claims.aud -cne 'https://oauth2.googleapis.com/token'){throw 'Unsafe synthetic token claims'}
                return @{status= $(if($state.case -eq 'token-rejected'){401}else{200});json=@{access_token='synthetic-token';token_type='Bearer'}}
            }
            if($request.method -eq 'GET') {
                $state.gets++
                $meta=@{id=$id;name='existing-name.zip';mimeType='application/zip';size=$manifest.size;md5Checksum=(Get-FileHash $zip -Algorithm MD5).Hash.ToLowerInvariant();sha256Checksum=$manifest.sha256;appProperties=@{wallet_studio_build_id=$(if($state.uploaded){'5'}elseif($state.case -eq 'newer-build'){'6'}else{'4'})};version='1';trashed=$false;capabilities=@{canEdit=$true}}
                if($state.case -in @('bootstrap-reviewed','bootstrap-unknown') -and -not $state.uploaded){$meta.appProperties=@{}}
                if($state.case -eq 'wrong-id'){$meta.id='another-file'}
                if($state.case -eq 'changed-version' -and $state.gets -eq 2){$meta.version='2'}
                if($state.case -eq 'bad-readback' -and $state.uploaded){$meta.sha256Checksum='0'*64}
                return @{status=200;headers=@{ETag=$(if($state.case -eq 'missing-etag'){$null}else{'"fixture-etag"'})};json=$meta}
            }
            if($request.method -eq 'PATCH') {
                $metadata=$request.body|ConvertFrom-Json
                if($metadata.name -or $metadata.permissions -or $metadata.parents -or $metadata.appProperties.wallet_studio_build_id -ne '5' -or -not $request.headers['If-Match']){throw 'Unexpected Drive metadata mutation'}
                return @{status=200;headers=@{Location=$(if($state.case -eq 'unsafe-session'){'https://attacker.example/upload'}else{"https://www.googleapis.com/upload/drive/v3/files/${id}?upload_id=synthetic"})}}
            }
            if($request.method -eq 'PUT') {
                if($request.file -cne $zip -or (Get-FileHash $request.file -Algorithm SHA256).Hash.ToLowerInvariant() -cne $manifest.sha256){throw 'Wrong upload bytes'}
                $state.uploaded=$true;return @{status=$(if($state.case -eq 'upload-failed'){500}else{200})}
            }
            throw 'Unexpected HTTP verb'
        }.GetNewClosure()
        $channel={$state.checks++;if($state.case -eq 'main-changed' -and $state.checks -ge 2){throw 'Synthetic main moved'}}.GetNewClosure()
        $failed=$false
        try {Invoke-DriveFileSync $manifest $zip $(if($case -eq 'invalid-id'){'bad/id'}else{$id}) $credential $http $channel $(if($case -eq 'bootstrap-reviewed'){$manifest.sha256}else{$null})|Out-Null}catch{$failed=$true}
        if($failed -ne ($case -notin @('valid','bootstrap-reviewed'))){throw "Drive classification failed: $case"}
        $puts=@($state.requests|Where-Object method -eq 'PUT')
        if($case -notin @('valid','bootstrap-reviewed','upload-failed','bad-readback') -and $puts.Count){throw "Unsafe fixture uploaded bytes: $case"}
        if($case -eq 'valid' -and ($puts.Count -ne 1 -or $state.gets -ne 3)){throw 'Valid existing-file update was not verified'}
        if(@($state.requests|Where-Object{$_.method -eq 'DELETE' -or ($_.method -eq 'POST' -and $_.url -ne 'https://oauth2.googleapis.com/token')}).Count){throw 'Create/delete forbidden'}
        $passed++
    }
    Write-Output "Drive sync: $passed adapter cases passed; fake HTTP/credentials, no production API."
}finally{
    $key.Dispose();$resolved=[IO.Path]::GetFullPath($testRoot);$temp=[IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')+'\'
    if(-not $resolved.StartsWith($temp,[StringComparison]::OrdinalIgnoreCase) -or -not ([IO.Path]::GetFileName($resolved)).StartsWith('wallet-drive-test-')){throw 'Unsafe test cleanup'}
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
