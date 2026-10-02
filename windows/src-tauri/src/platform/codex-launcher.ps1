# Aj verified this registered-protocol launch visibly opens the installed app.
# UUID validation and exact thread URL construction happen in windows.rs.
$ErrorActionPreference = 'Stop'
$url = '__COUCOU_THREAD_URL__'
Start-Process -FilePath $url -ErrorAction Stop
