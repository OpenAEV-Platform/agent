# Checks that the release keys the installers embed can actually be used by
# the runtime the installers run on.
#
# Importing an RSA key through RSACryptoServiceProvider depends on the
# cryptographic provider Windows picks, and the installers deliberately
# swallow an import failure so the next trusted key still gets a chance. A key
# that cannot be imported would therefore surface as "signature does not match
# any trusted release key", which sends people looking in the wrong place.
# This script turns that silent case into a build failure.
#
# Run it under both Windows PowerShell 5.1 and PowerShell 7: customers have
# 5.1, CI defaults to 7, and they do not use the same crypto stack.

$ErrorActionPreference = 'Stop'

# A fixed vector signed with a throwaway key, kept here so the check needs no
# secret. It proves this runtime can import a key of the size we ship and
# verify an RSA/SHA-256 signature produced by OpenSSL.
$vectorKeyXml = '<RSAKeyValue><Modulus>tJySP+e7NyoAmlqsxqNB+0Y8sfoCp+OenC7gB2X+dGIqZMUyRoNrs4CSUsejwQNTBTs8RM/MqlPBBKqYypoSLUqicwICPxaBpyRcwbivUUowWcF7KCqG1bvej4hmktD+IbuuhTTXZvhpd8kJlhsS4pUkk/73B+z+f9utGdaYj0UM5h06LVX7x6YJs49J3jM6MissrYb2wlUBhNidPVKD8eqLYEUZ5exg8QIJqXKaTXokk6kx/7VmZuK6snVidHwWAQOuxvn7Vyjs0MGcpmx8a5yJOx/j9FqNuVXzc3pV7dlgFl3KYdb8HXV9L5a+aLV+UehVHnqJAhG1J5rvMjTlVYx4AFpq0BSGRfuLz/eRQBcH89ab7m0Ww0HdOzoNuUBeadKXgLbwAQJ4DOc4sjWXMKVvnxirNwkDGpbtM/aSY239p4uzMaoClBXJFu9JyaDExz4O5iaHp2st27UV9cFmgnjyAgT7VxzUXA1pMDJUmtyH3B+E1DQ1H1nbN+GUf/ctJVMrQaUTbe5O4h8/SyMUQQFRg5XXi0DozHHWK5SVUZheqDn2cNOkQyXvTKYWgIR/8S5aMPd+6TyIC/mn2Qe94oXPce1HidRa6LaTMdGjoLmmiSd35TO+WMdBG02Hbn0A7F8JSFvZKFQtsjivhPRW3a3TYyoCuf7olyjbnzPpzEE=</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>'
$vectorSignature = 'RakRTaSxxo2kKyqliGb1tByY9YQqepsecefTSRblBRtOs1CU0Kw1jEDy8Dda6lB9mFnyBrxrzrc2ZsV07Ni4eEogOz+zCPCyINw21EW7oMEDVy4yh5cMo3vrQ6TISpEB/hGZr+Z7c1uBDEmRp71QY93u2qATJEVSaT7kCui3r2NiZiWdFzSM0ZOy5x6DxRnrgPn3rBcKF/s2gecL4kMkNycJ4WkdyFVw4ShL89k4txI2ftJSyKJ32yrf0xgh4roajc3+LmcvugdYQk9+Maci3JzuSXpaQSX4yH3+tpiveh4GYkkUuGskgvkcC1q2nWGuu24n83fCOLQdnaFHjspkXLzYGYegRCNv5C3Z4ID/pu7YlTymkf8wkC7AKr/zcE0uitj9qs+0g159NlL3xAU31bY9XH91Ck0ilZxYLTkfwYujjQZHkG9J0cZ9mbh9lrN1KDhYFYIOb+Cqm8BfmuvdyTbHWVZ3HbU4EtKxqEZEaeHEVhDmXpnhX81+vgWl9Enq+DE3A44CCFsMNbDeS/U2bWlMSCH3NVh3vUqHzyYyODOkkh3ctBfP3PORKwH0EquN2kJxqglKGaJvQOAqrcSVcBe6S9+d/io8OfMqC3lTlHS42dxaOjtf4tFVxX90hc2O1D0xDfz4hS6aQ74FHIkwnwp3Rq7xcHelYmVaXuDq/E8='
$vectorPayload = 'openaev-release-key-selftest'

$failures = 0

Write-Host '== import and verify a known OpenSSL signature =='
$vectorKey = New-Object System.Security.Cryptography.RSACryptoServiceProvider
$vectorKey.FromXmlString($vectorKeyXml)
$payloadBytes = [System.Text.Encoding]::UTF8.GetBytes($vectorPayload)
$signatureBytes = [Convert]::FromBase64String($vectorSignature)
if ($vectorKey.VerifyData($payloadBytes, 'SHA256', $signatureBytes))
{
    Write-Host "  ok, $($vectorKey.KeySize)-bit key imported and signature verified"
}
else
{
    Write-Host '  FAILED: a valid OpenSSL signature was rejected'
    $failures++
}

Write-Host '== every key embedded in the installers can be imported =='
$pattern = '<RSAKeyValue><Modulus>[^<]+</Modulus><Exponent>[^<]+</Exponent></RSAKeyValue>'
$scripts = Get-ChildItem -Path $PSScriptRoot -Filter 'agent-*.ps1' | Sort-Object Name
foreach ($script in $scripts)
{
    $content = [System.IO.File]::ReadAllText($script.FullName)
    $matched = [regex]::Matches($content, $pattern)
    if ($matched.Count -eq 0)
    {
        Write-Host "  FAILED: $($script.Name) embeds no release key"
        $failures++
        continue
    }

    foreach ($entry in $matched)
    {
        try
        {
            $key = New-Object System.Security.Cryptography.RSACryptoServiceProvider
            $key.FromXmlString($entry.Value)
            Write-Host "  ok, $($script.Name): $($key.KeySize)-bit key"
        }
        catch
        {
            Write-Host "  FAILED: $($script.Name) embeds a key this runtime cannot import: $_"
            $failures++
        }
    }
}

if ($failures -gt 0)
{
    Write-Host "$failures check(s) failed."
    exit 1
}

Write-Host 'All release keys are usable on this runtime.'
