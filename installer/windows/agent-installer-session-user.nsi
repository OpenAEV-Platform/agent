;includes
!include nsDialogs.nsh
!include LogicLib.nsh
!include FileFunc.nsh
!include StrFunc.nsh
${Using:StrFunc} StrRep
${Using:StrFunc} StrCase

!insertmacro GetParameters
!insertmacro GetOptions

!define APPNAME "OBAS Agent"
!define COMPANYNAME "Filigran"
!define DESCRIPTION "Filigran's agent for OpenBAS"
# These will be displayed by the "Click here for support information" link in "Add/Remove Programs"
# It is possible to use "mailto:" links in here to open the email client
!define HELPURL "https://filigran.io/" # "Support Information" link
!define UPDATEURL "https://filigran.io/" # "Product Updates" link
!define ABOUTURL "https://filigran.io/" # "Publisher" link
 
RequestExecutionLevel user
 
# rtf or txt file - remember if it is txt, it must be in the DOS text format (\r\n)
LicenseData "license.txt"
# This will be in the installer/uninstaller's title bar
Name "${COMPANYNAME} - ${APPNAME}"
Icon "openbas.ico"
outFile "agent-installer-session-user.exe"
 
; page definition
page license
page directory
Page custom nsDialogsConfig nsDialogsPageLeave
Page instfiles

!macro VerifyUserIsAdmin
UserInfo::GetAccountType
pop $0
${If} $0 != "admin" ;Require admin rights on NT4+
        messageBox mb_iconstop "Administrator rights required!"
        setErrorLevel 740 ;ERROR_ELEVATION_REQUIRED
        quit
${EndIf}
!macroend

Var Dialog
Var LabelURL
Var /GLOBAL ConfigURL
Var LabelToken
Var /GLOBAL ConfigToken
Var LabelUnsecuredCertificate
Var /GLOBAL ConfigUnsecuredCertificate
Var LabelWithProxy
Var /GLOBAL ConfigWithProxy
Var /GLOBAL ConfigServiceName
Var /GLOBAL ConfigInstallDir
Var /GLOBAL ConfigWithAdminPrivilege
Var /GLOBAL UserSanitized
Var /GLOBAL AgentName

function verifyParam

  ; check values are defined
  ${If} $ConfigURL == ""
    MessageBox MB_OK|MB_ICONEXCLAMATION "Missing URL"
	  Abort
  ${EndIf}

  ${If} $ConfigToken == ""
    MessageBox MB_OK|MB_ICONEXCLAMATION "Missing Token"
	  Abort
  ${EndIf}

  ${If} $ConfigUnsecuredCertificate != "false"
  ${AndIf} $ConfigUnsecuredCertificate != "true"
    MessageBox MB_OK|MB_ICONEXCLAMATION "Missing false or true value for unsecured certificate"
  	  Abort
  ${EndIf}

  ${If} $ConfigWithProxy != "false"
  ${AndIf} $ConfigWithProxy != "true"
    MessageBox MB_OK|MB_ICONEXCLAMATION " Missing false or true value for env with proxy"
      Abort
  ${EndIf}

functionEnd

function .onInit
    setShellVarContext all
    ${GetParameters} $R0
    ${GetOptions} $R0 ~OPENBAS_URL= $ConfigURL
    ${GetOptions} $R0 ~ACCESS_TOKEN= $ConfigToken
    ${GetOptions} $R0 ~UNSECURED_CERTIFICATE= $ConfigUnsecuredCertificate
    ${GetOptions} $R0 ~WITH_PROXY= $ConfigWithProxy
    ${GetOptions} $R0 ~SERVICE_NAME= $ConfigServiceName
    ${GetOptions} $R0 ~INSTALL_DIR= $ConfigInstallDir
    ${If} $ConfigServiceName == ""
        StrCpy $ConfigServiceName "OBASAgent-Session"
    ${EndIf}
    ${If} $ConfigInstallDir == ""
        StrCpy $ConfigInstallDir "C:\${COMPANYNAME}"
    ${EndIf}

    ;get the user name and sanitize it
    nsExec::ExecToStack 'cmd /c whoami'
    Pop $0
    Call sanitizeUserName
    Call trim
    pop $UserSanitized

    ;get the permission level
    Call checkIfElevated

    ; If running silently, check params and update install path with user name
    ${If} ${Silent}
        Call verifyParam
        Call updateInstallDir
    ${EndIf}

functionEnd

Var ConfigURLForm
Var ConfigTokenForm
Var ConfigUnsecuredCertificateForm
Var ConfigWithProxyForm

Function checkIfElevated
  ; Get the account type of the current process
  UserInfo::GetAccountType
  Pop $0

  ${If} $0 == "admin"
    StrCpy $ConfigWithAdminPrivilege "true"
  ${Else}
    StrCpy $ConfigWithAdminPrivilege "false"
  ${EndIf}
FunctionEnd

Function nsDialogsConfig
  ; disable next button
  GetDlgItem $0 $HWNDPARENT 1
  EnableWindow $0 0

	nsDialogs::Create 1018
	Pop $Dialog

	${If} $Dialog == error
		Abort
	${EndIf}

  ${NSD_CreateLabel} 0 0 100% 12u "OpenBAS URL *"
	Pop $LabelURL
	${NSD_CreateText} 0 12u 100% 12u "http://localhost:3001"
	Pop $ConfigURLForm
  ${NSD_CreateLabel} 0 24u 100% 12u "Access token *"
	Pop $LabelToken
	${NSD_CreatePassword} 0 36u 100% 12u ""
	Pop $ConfigTokenForm
  ${NSD_CreateLabel} 0 48u 100% 12u "Unsecured certificate (true or false) *"
	Pop $LabelUnsecuredCertificate
	${NSD_CreateText} 0 60u 100% 12u "false"
	Pop $ConfigUnsecuredCertificateForm
  ${NSD_CreateLabel} 0 72u 100% 12u " Env with proxy (true or false) *"
	Pop $LabelWithProxy
	${NSD_CreateText} 0 84u 100% 12u "false"
	Pop $ConfigWithProxyForm

  ${NSD_OnChange} $ConfigURLForm onFieldChange
  ${NSD_OnChange} $ConfigTokenForm onFieldChange
  ${NSD_OnChange} $ConfigUnsecuredCertificateForm onFieldChange
  ${NSD_OnChange} $ConfigWithProxyForm onFieldChange
  nsDialogs::Show

FunctionEnd

Function onFieldChange
  ; save in register the values entered by user
  ${NSD_GetText} $ConfigURLForm $ConfigURL
  ${NSD_GetText} $ConfigTokenForm $ConfigToken
  ${NSD_GetText} $ConfigUnsecuredCertificateForm $ConfigUnsecuredCertificate
  ${NSD_GetText} $ConfigWithProxyForm $ConfigWithProxy

  ; enable next button if both defined 
  ${If} $ConfigURL != "" 
  ${AndIf} $ConfigToken != ""
  ${AndIf} $ConfigUnsecuredCertificate != ""
  ${AndIf} $ConfigWithProxy != ""
    GetDlgItem $0 $HWNDPARENT 1
    EnableWindow $0 1
  ${Else}
    GetDlgItem $0 $HWNDPARENT 1
    EnableWindow $0 0
  ${EndIf}

FunctionEnd


Function updateInstallDir
  ${If} $ConfigWithAdminPrivilege == "true"
    StrCpy $AgentName "$ConfigServiceName-Administrator-$UserSanitized"
  ${Else}
    StrCpy $AgentName "$ConfigServiceName-$UserSanitized"
  ${EndIf}

  StrCpy $INSTDIR "$ConfigInstallDir\$AgentName"
FunctionEnd

Function nsDialogsPageLeave
  Call verifyParam
  Call updateInstallDir
FunctionEnd

; Trim
;   Removes leading & trailing whitespace from a string
; Usage:
;   Push
;   Call Trim
;   Pop
Function Trim
  Exch $R1 ; Original string
  Push $R2

  Loop:
    StrCpy $R2 "$R1" 1
	StrCmp "$R2" " " TrimLeft
	StrCmp "$R2" "$\r" TrimLeft
	StrCmp "$R2" "$\n" TrimLeft
	StrCmp "$R2" "$\t" TrimLeft
	GoTo Loop2
  TrimLeft:
	StrCpy $R1 "$R1" "" 1
	Goto Loop

  Loop2:
	StrCpy $R2 "$R1" 1 -1
	StrCmp "$R2" " " TrimRight
	StrCmp "$R2" "$\r" TrimRight
	StrCmp "$R2" "$\n" TrimRight
	StrCmp "$R2" "$\t" TrimRight
	GoTo Done
  TrimRight:
	StrCpy $R1 "$R1" -1
	Goto Loop2

  Done:
	Pop $R2
	Exch $R1
FunctionEnd

Function sanitizeUserName
  Exch $0  ; get the username from the stack
  ; Replace  characters with an empty string
  ${StrRep} $0 $0 "/" ""
  ${StrRep} $0 $0 "\" ""
  ${StrRep} $0 $0 ":" ""
  ${StrRep} $0 $0 "*" ""
  ${StrRep} $0 $0 "?" ""
  ${StrRep} $0 $0 "<" ""
  ${StrRep} $0 $0 ">" ""
  ${StrRep} $0 $0 "|" ""

  ; Convert to lowercase
  ${StrCase} $0 $0 "L"

  Exch $0
FunctionEnd

section "install"

  # Files for the install directory - to build the installer, these should be in the same directory as the install script (this file)
  setOutPath $INSTDIR
  # Files added here should be removed by the uninstaller (see section "uninstall")
  file "..\..\target\release\openbas-agent.exe"
  file "openbas.ico"
	
  ; write agent config file
  FileOpen $4 "$INSTDIR\openbas-agent-config.toml" w
    FileWrite $4 "debug=false$\r$\n"
    FileWrite $4 "$\r$\n"
    FileWrite $4 "[openbas]$\r$\n"
    FileWrite $4 "url = $\"$ConfigURL$\"$\r$\n"
    FileWrite $4 "token = $\"$ConfigToken$\"$\r$\n"
    FileWrite $4 "unsecured_certificate = $ConfigUnsecuredCertificate$\r$\n"
    FileWrite $4 "with_proxy = $ConfigWithProxy$\r$\n"
    FileWrite $4 "installation_mode = $\"session-user$\"$\r$\n"
    FileWrite $4 "service_name = $\"$ConfigServiceName$\"$\r$\n"
    FileWrite $4 "service_full_name = $\"$AgentName$\"$\r$\n"
    FileWrite $4 "$\r$\n" ; newline
  FileClose $4

  ; write agent start file to launch the agent without a powershell window displayed
  FileOpen $4 "$INSTDIR\openbas_agent_start.ps1" w
    FileWrite $4 "Start-Process -FilePath '$INSTDIR\openbas-agent.exe' -WindowStyle Hidden"
  FileClose $4

  ;admin -> use a scheduled task, non admin -> write in the registry and create a script to launch the agent to create a startup app
  ${If} $ConfigWithAdminPrivilege == "true"
    ; Stop the scheduled task
    ExecWait 'schtasks /End /TN "$AgentName"' $0

    ; Remove the existing scheduled task if it exists
    ExecWait 'schtasks /Delete /TN "$AgentName" /F' $0

    ; Create the task
    ExecWait 'schtasks /Create /TN "$AgentName" /TR "powershell.exe -ExecutionPolicy Bypass -WindowStyle hidden -File $INSTDIR\openbas_agent_start.ps1" /SC ONLOGON /RL HIGHEST' $0

    ;start the task
    ExecWait 'schtasks /Run /TN "$AgentName"' $0
  ${Else}

    SetRegView 64

    ; Remove registry entry
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "$AgentName"

    ;Write in the registry to start the agent at logon
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "$AgentName" "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -ExecutionPolicy Bypass -WindowStyle hidden -File $INSTDIR\openbas_agent_start.ps1"

    ;Start the agent
    nsExec::ExecToStack "powershell.exe -ExecutionPolicy Bypass -WindowStyle hidden -File $INSTDIR\openbas_agent_start.ps1"
  ${EndIf}

  # Uninstaller - See function un.onInit and section "uninstall" for configuration
  writeUninstaller "$INSTDIR\uninstall.exe"
sectionEnd
 
# Uninstaller
Function un.ReadServiceNameFromToml
    StrCpy $AgentName "" ; Reset
    FileOpen $0 "$INSTDIR\openbas-agent-config.toml" r

read_loop:
    FileRead $0 $1
    StrCmp $1 "" close_file ; End of file

    ; Remove trailing newline/carriage return
    Push $1
    Call un.StripNewline
    Pop $1

    ; Skip empty lines
    StrCmp $1 "" read_loop

    ; Debug: Show every cleaned line being read (optional - can be removed)
    ; MessageBox MB_OK "Line: '$1'"

    ; Trim leading spaces
trim_spaces:
    StrCpy $2 $1 1
    StrCmp $2 " " 0 after_trim
    StrCpy $1 $1 "" 1
    Goto trim_spaces

after_trim:
    ; See if this is the service_full_name line
    StrCpy $2 $1 12
    StrCmp $2 "service_full_name" match
    Goto read_loop

match:
    ; Now, extract value after '='
    StrCpy $3 $1
    StrCpy $4 0

find_eq:
    StrCpy $5 $3 1 $4
    StrCmp $5 "" read_loop  ; If no '=' found, continue to next line
    StrCmp $5 "=" found_eq
    IntOp $4 $4 + 1
    Goto find_eq

found_eq:
    IntOp $4 $4 + 1
    StrCpy $6 $3 "" $4 ; everything after '='

    ; Remove leading space(s)
trim_val:
    StrCpy $7 $6 1
    StrCmp $7 " " 0 trim_val_done
    StrCpy $6 $6 "" 1
    Goto trim_val

trim_val_done:
    ; Remove quotes if present
    StrCpy $7 $6 1
    StrCmp $7 '"' 0 check_end_quote
    StrCpy $6 $6 "" 1

check_end_quote:
    StrLen $8 $6
    IntCmp $8 0 skip_quote skip_quote
    IntOp $8 $8 - 1
    StrCpy $7 $6 1 $8
    StrCmp $7 '"' 0 skip_quote
    StrCpy $6 $6 $8

skip_quote:
    StrCpy $AgentName $6
    ; MessageBox MB_OK "Extracted aAgentName: '$AgentName'"
    Goto close_file

close_file:
    FileClose $0
FunctionEnd

Function un.StripNewline
    Exch $0
    Push $1
    Push $2

again:
    StrLen $2 $0
    IntCmp $2 0 done done
    IntOp $2 $2 - 1
    StrCpy $1 $0 1 $2
    StrCmp $1 "$\r" strip
    StrCmp $1 "$\n" strip
    Goto done

strip:
    StrCpy $0 $0 $2
    Goto again

done:
    Pop $2
    Pop $1
    Exch $0
FunctionEnd

function un.onInit
	SetShellVarContext all

   # Get ServiceName
   Call un.ReadServiceNameFromToml

	#Verify the uninstaller - last chance to back out
	MessageBox MB_OKCANCEL "Permanently remove ${APPNAME}?" IDOK continue
		Abort
	continue:
functionEnd

Function un.StrContains
   Exch $R1  ; Substring to search for
   Exch
   Exch $R2  ; String to search in
   Push $R3
   Push $R4
   Push $R5

   StrLen $R3 $R1      ; $R3 = len(substring)
   StrLen $R4 $R2      ; $R4 = len(string)
   IntOp $R5 $R4 - $R3 ; $R5 = max start index

   StrCpy $R0 0

loop:
   IntCmp $R0 $R5 found notfound found
   StrCpy $R6 $R2 $R3 $R0 ; $R6 = substring from position $R0
   StrCmp $R6 $R1 found
   IntOp $R0 $R0 + 1
   Goto loop

notfound:
   StrCpy $R1 ""
   Goto done

found:
   StrCpy $R1 "1"
   Goto done

done:
   Pop $R5
   Pop $R4
   Pop $R3
   Pop $R2
   Exch $R1
FunctionEnd
 
section "uninstall"
   Push $AgentName
   Push "Administrator"
   Call un.StrContains
   Pop $R0

   ${If} $R0 != ""
    !insertmacro VerifyUserIsAdmin
     ; Stop the scheduled task
     ExecWait 'schtasks /End /TN "$AgentName"' $0
     ; Remove the existing scheduled task if it exists
     ExecWait 'schtasks /Delete /TN "$AgentName" /F' $0
   ${Else}
     ;process kill is done in the powershell script

     SetRegView 64
     ; Remove registry entry
     DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "$AgentName"
   ${EndIf}

  ; Wait 1s to allow the task to fully end before deleting the exe
  Sleep 1000

  ; delete everything
  RMDir /r $INSTDIR
sectionEnd
