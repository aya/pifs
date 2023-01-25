#!/bin/sh
#$1 vaut exec si on veut lancer le programme apres avoir fait le menage
#$2 est le nom du programme qui tourne (pifs)
#$3 est le chemin du dossier qui est monte (/home/max/tmp)
#exemple de commande
#./pifs -o mdd=/home/max/mdd  --erreur=/media/max/LINUX/Yann/pifs_max/espion_fuse.txt /home/max/tmp
#./menage_pifs.sh nexec ./pifs /home/max/tmp

fichier_log="/media/max/LINUX/Yann/pifs_max/espion_fuse.txt"
mount |grep $2
chaine=$(ps -aux | grep "$2 -o" |cut -c10-18)
#echo $chaine
bidule=$(echo $chaine | cut -d ' ' -f 1)
echo "on fait kill -9 "$bidule" |sudo umount -fl "$3
kill -9 $bidule |sudo umount -fl $3
mount |grep $2
chaine2="-Wall "$2".c -I/usr/include/fuse3 -lfuse3 -lpthread -o "$2
gcc $chaine2
if [ $1 = "exec" ];then
echo "lancement de ./"$2" -o mdd=/home/max/mdd  --erreur="$fichier_log $3
`./$2 -o mdd=/home/max/mdd --erreur=$fichier_log $3`
fi
